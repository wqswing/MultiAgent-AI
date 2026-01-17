//! ReAct loop implementation.
//!
//! ReAct (Reason + Act) is the core control loop for the agent:
//! 1. Reason about the current state
//! 2. Choose an action (tool or respond)
//! 3. Execute the action
//! 4. Observe the result
//! 5. Repeat until done or max iterations
//!
//! v0.2 Autonomous Capabilities:
//! - Dynamic Context Compression (auto-compresses when token threshold exceeded)
//! - Subagent Delegation (allows spawning child agents for subtasks)

use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use mutilAgent_core::{
    traits::{ArtifactStore, ChatMessage, Controller, LlmClient, LlmResponse, ToolRegistry, SessionStore},
    types::{AgentResult, HistoryEntry, Session, SessionStatus, TaskState, TokenUsage, UserIntent, ToolCallInfo},
    Error, Result,
};

use crate::context::{ContextCompressor, CompressionConfig, TruncationCompressor};
use crate::delegation::{Delegator, DelegationRequest};

/// ReAct controller configuration.
#[derive(Debug, Clone)]
pub struct ReActConfig {
    /// Maximum iterations before giving up.
    pub max_iterations: usize,
    /// Default token budget.
    pub default_budget: u64,
    /// Enable state persistence.
    pub persist_state: bool,
    /// Temperature for LLM calls.
    pub temperature: f32,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            default_budget: 50_000,
            persist_state: true,
            temperature: 0.7,
        }
    }
}

/// Parsed action from LLM response.
#[derive(Debug, Clone)]
pub enum ReActAction {
    /// Call a tool with arguments.
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    /// Final answer - task complete.
    FinalAnswer(String),
    /// Continue thinking (no action yet).
    Think(String),
    /// Delegate to a subagent (v0.2 autonomous capability).
    Delegate {
        objective: String,
        context: String,
    },
}

/// ReAct controller for executing complex tasks.
pub struct ReActController {
    /// Configuration.
    config: ReActConfig,
    /// LLM client for reasoning.
    llm: Option<Arc<dyn LlmClient>>,
    /// Tool registry for actions.
    tools: Option<Arc<dyn ToolRegistry>>,
    /// Artifact store for large outputs.
    store: Option<Arc<dyn ArtifactStore>>,
    /// Session store for persistence.
    session_store: Option<Arc<dyn SessionStore>>,
    /// Context compressor for long sessions (v0.2).
    compressor: Option<Arc<dyn ContextCompressor>>,
    /// Compression configuration.
    compression_config: CompressionConfig,
    /// Delegator for subagent spawning (v0.2).
    delegator: Option<Arc<dyn Delegator>>,
}

impl ReActController {
    /// Create a new ReAct controller.
    pub fn new(config: ReActConfig) -> Self {
        Self {
            config,
            llm: None,
            tools: None,
            store: None,
            session_store: None,
            compressor: Some(Arc::new(TruncationCompressor::new())), // Default compressor
            compression_config: CompressionConfig::default(),
            delegator: None,
        }
    }

    /// Set the LLM client.
    pub fn with_llm(mut self, llm: Arc<dyn LlmClient>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// Set the tool registry.
    pub fn with_tools(mut self, tools: Arc<dyn ToolRegistry>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the artifact store.
    pub fn with_store(mut self, store: Arc<dyn ArtifactStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Set the session store.
    pub fn with_session_store(mut self, session_store: Arc<dyn SessionStore>) -> Self {
        self.session_store = Some(session_store);
        self
    }

    /// Set the context compressor (v0.2 autonomous capability).
    pub fn with_compressor(mut self, compressor: Arc<dyn ContextCompressor>) -> Self {
        self.compressor = Some(compressor);
        self
    }

    /// Set compression configuration.
    pub fn with_compression_config(mut self, config: CompressionConfig) -> Self {
        self.compression_config = config;
        self
    }

    /// Set the delegator for subagent spawning (v0.2 autonomous capability).
    pub fn with_delegator(mut self, delegator: Arc<dyn Delegator>) -> Self {
        self.delegator = Some(delegator);
        self
    }

    /// Create a new session.
    fn create_session(&self, goal: &str) -> Session {
        Session {
            id: Uuid::new_v4().to_string(),
            status: SessionStatus::Running,
            history: vec![HistoryEntry {
                role: "system".to_string(),
                content: self.build_system_prompt(goal),
                tool_call: None,
                timestamp: chrono_timestamp(),
            }],
            task_state: Some(TaskState {
                iteration: 0,
                goal: goal.to_string(),
                observations: Vec::new(),
                pending_actions: Vec::new(),
            }),
            token_usage: TokenUsage::with_budget(self.config.default_budget),
            created_at: chrono_timestamp(),
            updated_at: chrono_timestamp(),
        }
    }

    /// Build the system prompt for the agent.
    fn build_system_prompt(&self, goal: &str) -> String {
        let tools_description = self.get_tools_description();
        
        format!(
            r#"You are an AI assistant that uses the ReAct (Reasoning + Acting) pattern.

GOAL: {goal}

AVAILABLE TOOLS:
{tools_description}

INSTRUCTIONS:
1. Think step by step about what needs to be done
2. Use tools when needed by responding with ACTION
3. After receiving tool results, continue reasoning
4. When done, provide your FINAL ANSWER

RESPONSE FORMAT:
Use exactly one of these formats in each response:

For thinking/reasoning:
THOUGHT: <your reasoning here>

For tool calls:
ACTION: <tool_name>
ARGS: <json arguments>

For final answer (when task is complete):
FINAL ANSWER: <your complete answer>

Always think before acting. Be concise and focused on the goal."#
        )
    }



    /// Get description of available tools (for system prompt building).
    fn get_tools_description(&self) -> String {
        // For the system prompt, we return a placeholder since we can't call async here.
        // The actual tools list is fetched async when executing.
        "Tools will be loaded when execution starts.".to_string()
    }

    /// Build chat messages from session history.
    fn build_messages(&self, session: &Session) -> Vec<ChatMessage> {
        session
            .history
            .iter()
            .map(|entry| ChatMessage {
                role: entry.role.clone(),
                content: entry.content.clone(),
                tool_calls: None,
            })
            .collect()
    }

    /// Parse the LLM response to extract action.
    fn parse_action(&self, response: &str) -> ReActAction {
        let response_trimmed = response.trim();

        // Check for FINAL ANSWER
        if let Some(answer) = response_trimmed.strip_prefix("FINAL ANSWER:") {
            return ReActAction::FinalAnswer(answer.trim().to_string());
        }

        // Check for ACTION + ARGS pattern
        if response_trimmed.contains("ACTION:") {
            if let Some((_action_part, rest)) = response_trimmed.split_once("ACTION:") {
                let action_line = rest.lines().next().unwrap_or("").trim();
                
                // Look for ARGS
                let args = if let Some(args_pos) = rest.find("ARGS:") {
                    let args_str = &rest[args_pos + 5..];
                    let args_line = args_str.lines().next().unwrap_or("{}").trim();
                    serde_json::from_str(args_line).unwrap_or(serde_json::json!({}))
                } else {
                    serde_json::json!({})
                };

                return ReActAction::ToolCall {
                    name: action_line.to_string(),
                    args,
                };
            }
        }

        // Check for THOUGHT
        if let Some(thought) = response_trimmed.strip_prefix("THOUGHT:") {
            return ReActAction::Think(thought.trim().to_string());
        }

        // Check for DELEGATE (v0.2 subagent pattern)
        if response_trimmed.contains("DELEGATE:") {
            if let Some((_delegate_part, rest)) = response_trimmed.split_once("DELEGATE:") {
                let objective = rest.lines().next().unwrap_or("").trim().to_string();
                let context = if let Some(ctx_pos) = rest.find("CONTEXT:") {
                    rest[ctx_pos + 8..].lines().next().unwrap_or("").trim().to_string()
                } else {
                    String::new()
                };
                return ReActAction::Delegate { objective, context };
            }
        }

        // Default: treat as thought
        ReActAction::Think(response_trimmed.to_string())
    }

    /// Execute a single ReAct iteration with LLM.
    async fn execute_iteration_with_llm(
        &self,
        session: &mut Session,
        iteration: usize,
    ) -> Result<Option<AgentResult>> {
        let llm = self.llm.as_ref().ok_or_else(|| {
            Error::controller("LLM client not configured")
        })?;

        tracing::info!(
            session_id = %session.id,
            iteration = iteration,
            history_len = session.history.len(),
            "Executing ReAct iteration"
        );

        // v0.2: Auto-compress context if threshold exceeded
        let mut messages = self.build_messages(session);
        if let Some(ref compressor) = self.compressor {
            if compressor.needs_compression(&messages, &self.compression_config) {
                tracing::info!("Context compression triggered - compressing history");
                let result = compressor.compress(messages, &self.compression_config).await?;
                messages = result.messages;
                tracing::info!(
                    messages_compressed = result.messages_compressed,
                    estimated_tokens = result.estimated_tokens,
                    "Context compressed"
                );
            }
        }

        // Call LLM with (possibly compressed) messages
        let response: LlmResponse = llm.chat(&messages).await?;

        // Update token usage
        session.token_usage.add(
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
        );

        tracing::debug!(
            response_len = response.content.len(),
            tokens_used = session.token_usage.total_tokens,
            "LLM response received"
        );

        // Add assistant response to history
        session.history.push(HistoryEntry {
            role: "assistant".to_string(),
            content: response.content.clone(),
            tool_call: None,
            timestamp: chrono_timestamp(),
        });

        // Parse and execute action
        let action = self.parse_action(&response.content);

        match action {
            ReActAction::FinalAnswer(answer) => {
                tracing::info!(answer_len = answer.len(), "Task completed with final answer");
                Ok(Some(AgentResult::Text(answer)))
            }

            ReActAction::ToolCall { name, args } => {
                tracing::info!(tool = %name, "Executing tool call");

                let observation = if let Some(ref tools) = self.tools {
                    match tools.execute(&name, args.clone()).await {
                        Ok(output) => {
                            if output.success {
                                format!("Tool '{}' succeeded:\n{}", name, output.content)
                            } else {
                                format!("Tool '{}' failed:\n{}", name, output.content)
                            }
                        }
                        Err(e) => format!("Tool '{}' error: {}", name, e),
                    }
                } else {
                    format!("Tool '{}' not available (no tools configured)", name)
                };

                // Add observation to history
                session.history.push(HistoryEntry {
                    role: "user".to_string(),
                    content: format!("OBSERVATION: {}", observation),
                    tool_call: Some(ToolCallInfo {
                        name: name.clone(),
                        arguments: args.clone(),
                        result: Some(observation.clone()),
                    }),
                    timestamp: chrono_timestamp(),
                });

                // Update task state
                if let Some(ref mut task_state) = session.task_state {
                    task_state.observations.push(observation);
                }

                Ok(None) // Continue loop
            }

            ReActAction::Think(thought) => {
                tracing::debug!(thought_len = thought.len(), "Agent thinking");
                
                // Ask the agent to take an action
                session.history.push(HistoryEntry {
                    role: "user".to_string(),
                    content: "Please take an action using a tool, or provide your FINAL ANSWER if the task is complete.".to_string(),
                    tool_call: None,
                    timestamp: chrono_timestamp(),
                });

                Ok(None) // Continue loop
            }

            ReActAction::Delegate { objective, context } => {
                // v0.2: Autonomous subagent delegation
                tracing::info!(objective = %objective, "Spawning subagent for delegation");

                let observation = if let Some(ref delegator) = self.delegator {
                    let request = DelegationRequest::new(&objective)
                        .with_context(&context);
                    
                    match delegator.delegate(request).await {
                        Ok(result) => {
                            if result.success {
                                format!("Subagent completed successfully:\n{}", result.result)
                            } else {
                                format!("Subagent failed: {}", result.error.unwrap_or_default())
                            }
                        }
                        Err(e) => format!("Delegation error: {}", e),
                    }
                } else {
                    format!("Delegation not available (no delegator configured). Objective: {}", objective)
                };

                // Add delegation result to history
                session.history.push(HistoryEntry {
                    role: "user".to_string(),
                    content: format!("DELEGATION RESULT: {}", observation),
                    tool_call: None,
                    timestamp: chrono_timestamp(),
                });

                // Update task state
                if let Some(ref mut task_state) = session.task_state {
                    task_state.observations.push(observation);
                }

                Ok(None) // Continue loop
            }
        }
    }

    /// Execute iteration (mock if no LLM, real if LLM configured).
    async fn execute_iteration(
        &self,
        session: &mut Session,
        iteration: usize,
    ) -> Result<Option<AgentResult>> {
        if self.llm.is_some() {
            self.execute_iteration_with_llm(session, iteration).await
        } else {
            // Mock implementation for testing without LLM
            tracing::info!(
                session_id = %session.id,
                iteration = iteration,
                "Executing ReAct iteration (mock - no LLM)"
            );

            Ok(Some(AgentResult::Text(format!(
                "Mock ReAct execution. Goal: {}. Configure LLM client for real execution.",
                session
                    .task_state
                    .as_ref()
                    .map(|t| t.goal.as_str())
                    .unwrap_or("unknown")
            ))))
        }
    }
}

#[async_trait]
impl Controller for ReActController {
    async fn execute(&self, intent: UserIntent) -> Result<AgentResult> {
        match intent {
            UserIntent::FastAction { tool_name, args } => {
                // Fast path: direct tool execution
                tracing::info!(tool = %tool_name, "Fast path execution");

                if let Some(ref tools) = self.tools {
                    match tools.execute(&tool_name, args).await {
                        Ok(output) => {
                            if output.success {
                                Ok(AgentResult::Text(output.content))
                            } else {
                                Ok(AgentResult::Error {
                                    message: output.content,
                                    code: "TOOL_ERROR".to_string(),
                                })
                            }
                        }
                        Err(e) => Ok(AgentResult::Error {
                            message: e.to_string(),
                            code: "TOOL_NOT_FOUND".to_string(),
                        }),
                    }
                } else {
                    // No tools available - mock response
                    Ok(AgentResult::Text(format!(
                        "Fast path: would execute tool '{}'. Tools not configured.",
                        tool_name
                    )))
                }
            }

            UserIntent::ComplexMission {
                goal,
                context_summary,
                visual_refs,
            } => {
                // Slow path: ReAct loop
                tracing::info!(
                    goal = %goal,
                    context_len = context_summary.len(),
                    refs_count = visual_refs.len(),
                    "Starting ReAct loop"
                );

                let mut session = self.create_session(&goal);

                // Add user context to history
                session.history.push(HistoryEntry {
                    role: "user".to_string(),
                    content: if visual_refs.is_empty() {
                        context_summary.clone()
                    } else {
                        format!("{}\n\nReferences: {:?}", context_summary, visual_refs)
                    },
                    tool_call: None,
                    timestamp: chrono_timestamp(),
                });

                // Execute ReAct loop
                for iteration in 0..self.config.max_iterations {
                    if let Some(ref mut task_state) = session.task_state {
                        task_state.iteration = iteration;
                    }

                    match self.execute_iteration(&mut session, iteration).await? {
                        Some(result) => {
                            session.updated_at = chrono_timestamp();
                            session.status = SessionStatus::Completed;
                            
                            // Persist final state
                            if self.config.persist_state {
                                if let Some(store) = &self.session_store {
                                    if let Err(e) = store.save(&session).await {
                                        tracing::warn!(error = %e, "Failed to save session state");
                                    }
                                }
                            }
                            
                            return Ok(result);
                        }
                        None => {
                            session.updated_at = chrono_timestamp();
                            
                            // Persist intermediate state
                            if self.config.persist_state {
                                if let Some(store) = &self.session_store {
                                    if let Err(e) = store.save(&session).await {
                                        tracing::warn!(error = %e, "Failed to save session state");
                                    }
                                }
                            }

                            // Check budget
                            if session.token_usage.is_exceeded() {
                                session.status = SessionStatus::Failed;
                                return Err(Error::BudgetExceeded {
                                    used: session.token_usage.total_tokens,
                                    limit: session.token_usage.budget_limit,
                                });
                            }
                            continue;
                        }
                    }
                }

                // Max iterations reached
                session.status = SessionStatus::Failed;
                Err(Error::MaxIterationsExceeded(self.config.max_iterations))
            }
        }
    }

    async fn resume(&self, session_id: &str) -> Result<AgentResult> {
        tracing::warn!(session_id = session_id, "Resume not yet implemented");
        Err(Error::controller("Resume not yet implemented - coming in Phase 2 persistence"))
    }

    async fn cancel(&self, session_id: &str) -> Result<()> {
        tracing::info!(session_id = session_id, "Cancel requested");
        // In full implementation, would mark session as cancelled in store
        Ok(())
    }
}

/// Get current timestamp.
fn chrono_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_final_answer() {
        let controller = ReActController::new(ReActConfig::default());
        
        let response = "FINAL ANSWER: The result is 42.";
        let action = controller.parse_action(response);
        
        match action {
            ReActAction::FinalAnswer(answer) => {
                assert_eq!(answer, "The result is 42.");
            }
            _ => panic!("Expected FinalAnswer"),
        }
    }

    #[test]
    fn test_parse_tool_call() {
        let controller = ReActController::new(ReActConfig::default());
        
        let response = r#"THOUGHT: I need to calculate something.
ACTION: calculator
ARGS: {"operation": "add", "a": 5, "b": 3}"#;
        
        let action = controller.parse_action(response);
        
        match action {
            ReActAction::ToolCall { name, args } => {
                assert_eq!(name, "calculator");
                assert_eq!(args["operation"], "add");
            }
            _ => panic!("Expected ToolCall, got {:?}", action),
        }
    }

    #[test]
    fn test_parse_thought() {
        let controller = ReActController::new(ReActConfig::default());
        
        let response = "THOUGHT: I need to think about this more.";
        let action = controller.parse_action(response);
        
        match action {
            ReActAction::Think(thought) => {
                assert!(thought.contains("think about"));
            }
            _ => panic!("Expected Think"),
        }
    }

    #[tokio::test]
    async fn test_fast_action() {
        let controller = ReActController::new(ReActConfig::default());

        let intent = UserIntent::FastAction {
            tool_name: "test_tool".to_string(),
            args: serde_json::json!({"query": "test"}),
        };

        let result = controller.execute(intent).await.unwrap();
        match result {
            AgentResult::Text(text) => {
                assert!(text.contains("test_tool"));
            }
            _ => panic!("Expected Text result"),
        }
    }

    #[tokio::test]
    async fn test_complex_mission_mock() {
        let controller = ReActController::new(ReActConfig::default());

        let intent = UserIntent::ComplexMission {
            goal: "Test goal".to_string(),
            context_summary: "Test context".to_string(),
            visual_refs: vec![],
        };

        let result = controller.execute(intent).await.unwrap();
        match result {
            AgentResult::Text(text) => {
                assert!(text.contains("Mock ReAct"));
            }
            _ => panic!("Expected Text result"),
        }
    }
}
