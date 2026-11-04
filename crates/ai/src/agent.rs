//! The turn loop: context in, tool calls executed, turns and usage out.
//!
//! One call to [`AgentLoop::run`] is one assistant turn. It:
//!
//! 1. announces the compiled context (its hash is what replay compares);
//! 2. asks the provider for an answer, streaming deltas to the caller;
//! 3. for every tool call the model makes, applies the permission policy and
//!    then the host: a denied call returns `denied`, a call that needs a person
//!    returns `approval_required`, an allowed call runs;
//! 4. feeds each result back to the model and asks again, up to
//!    [`TurnLimits::max_rounds`];
//! 5. stops early when the caller cancels — and, crucially, executes no further
//!    writes once cancelled.
//!
//! The loop owns no I/O of its own beyond the provider: board and study writes
//! go through [`ToolHost`], which the server implements over the repository.
//! That keeps the whole turn testable without a database, and keeps the one
//! place that is allowed to mutate a board in one object.
//!
//! Errors are results, not exceptions. A tool that refuses is fed back to the
//! model, which is usually the thing that can fix its own argument; a provider
//! that fails ends the turn with an [`ErrorInfo`] the transcript can render.

use std::collections::HashMap;

use serde_json::Value;

use crate::policy::{Origin, PermissionProfile, PolicyDecision};
use crate::protocol::{
    AiEventPayload, AiTurn, ErrorInfo, ModelRef, SessionMode, StopReason, ToolCall, ToolResult,
    TurnRole, Usage, result_code,
};
use crate::provider::{
    Cancellation, ChatMessage, CompletionRequest, Provider, ProviderEvent, ToolDefinition,
};
use crate::tools::ToolRegistry;

/// Stable codes the loop itself produces.
pub mod loop_code {
    /// The model used every allowed tool round without finishing.
    pub const ROUNDS_EXHAUSTED: &str = "agent_rounds_exhausted";
    /// The same tool kept failing; the loop stopped asking it again.
    pub const TOOL_RETRIES_EXHAUSTED: &str = "agent_tool_retries_exhausted";
    /// The model asked for a tool that is not registered.
    pub const UNKNOWN_TOOL: &str = "agent_unknown_tool";
}

/// The compiled context for one turn.
///
/// Produced by the context compiler and carried through to the stored turn, so
/// a transcript can be replayed and its hash compared byte for byte.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TurnContext {
    /// The context as the system message the model reads.
    pub system: String,
    /// Canonical JSON of the compiled context.
    pub context_json: String,
    /// SHA-256 of [`TurnContext::context_json`].
    pub context_sha256: String,
    pub spec_version: String,
    pub template_version: String,
    pub tool_schema_version: String,
    /// Engine binary fingerprint when analysis contributed to the context.
    pub engine_fingerprint: Option<String>,
    /// Estimated tokens of the compiled context.
    pub tokens_estimate: u32,
}

/// How far the loop may go before it gives up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnLimits {
    /// Provider rounds per turn, including the first. Each round may call tools.
    pub max_rounds: u32,
    /// How many times one tool may fail before the loop stops asking it.
    pub max_tool_retries: u32,
}

impl Default for TurnLimits {
    fn default() -> Self {
        Self {
            max_rounds: 6,
            max_tool_retries: 2,
        }
    }
}

/// Everything one turn needs.
#[derive(Debug, Clone)]
pub struct TurnPlan {
    pub session_id: String,
    /// Identifies this turn in the event stream, before it is stored.
    pub turn_id: String,
    /// The sequence number the stored turn will take; carried on `TurnStarted`.
    pub seq: u64,
    pub model: ModelRef,
    pub mode: SessionMode,
    pub origin: Origin,
    pub permissions: PermissionProfile,
    /// Conversation before this turn. System messages here are ignored in
    /// favour of the compiled context.
    pub history: Vec<ChatMessage>,
    /// The user's new message.
    pub user_message: String,
    pub context: TurnContext,
    pub limits: TurnLimits,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
}

impl TurnPlan {
    /// A chat turn over a compiled context.
    pub fn chat(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        model: ModelRef,
        context: TurnContext,
        user_message: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            seq: 0,
            model,
            mode: SessionMode::Chat,
            origin: Origin::InApp,
            permissions: PermissionProfile::default(),
            history: Vec::new(),
            user_message: user_message.into(),
            context,
            limits: TurnLimits::default(),
            temperature: 0.2,
            max_tokens: None,
        }
    }
}

/// The host side of tool execution: the one place that may mutate state.
pub trait ToolHost {
    /// Run one allowed call and report its outcome.
    ///
    /// Implementations must not return an `Err`: a refused write is a
    /// [`ToolResult`] with `ok: false` and a code from
    /// [`crate::protocol::result_code`], because the model needs to read it.
    fn call(&mut self, call: &ToolCall) -> ToolResult;

    /// Queue a person's approval for a call the policy will not auto-approve.
    ///
    /// Returns the approval id when a request was recorded, so the transcript
    /// can point at it. A host without a queue returns `None` and the call is
    /// simply refused — never executed.
    fn request_approval(&mut self, call: &ToolCall, summary: String) -> Option<String> {
        let _ = (call, summary);
        None
    }
}

/// One assistant turn produced by the loop.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnDraft {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<ToolResult>,
    pub usage: Usage,
    pub stop_reason: StopReason,
    pub error: Option<ErrorInfo>,
}

impl TurnDraft {
    /// The stored-turn shape. The store assigns the id and sequence.
    pub fn to_turn(&self, plan: &TurnPlan) -> AiTurn {
        AiTurn {
            id: String::new(),
            session_id: plan.session_id.clone(),
            seq: plan.seq,
            role: TurnRole::Assistant,
            content: self.content.clone(),
            reasoning: self.reasoning.clone(),
            tool_calls: self.tool_calls.clone(),
            tool_results: self.tool_results.clone(),
            context_json: plan.context.context_json.clone(),
            context_sha256: plan.context.context_sha256.clone(),
            spec_version: plan.context.spec_version.clone(),
            template_version: plan.context.template_version.clone(),
            tool_schema_version: plan.context.tool_schema_version.clone(),
            engine_fingerprint: plan.context.engine_fingerprint.clone(),
            model: plan.model.clone(),
            usage: self.usage,
            error: self.error.clone(),
            created_at: 0,
        }
    }
}

/// What one turn produced.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentOutcome {
    pub turn_id: String,
    /// The assistant turns, in the order they were produced. Usually one; a
    /// round that calls tools closes its own draft and the next round starts
    /// another.
    pub drafts: Vec<TurnDraft>,
    /// Every tool result in call order.
    pub tool_results: Vec<ToolResult>,
    /// Summed across rounds.
    pub usage: Usage,
    pub stop_reason: StopReason,
    pub error: Option<ErrorInfo>,
    /// How many provider rounds ran.
    pub rounds: u32,
    pub context: TurnContext,
}

impl AgentOutcome {
    /// The final assistant text, which is what a chat bubble shows.
    pub fn reply(&self) -> String {
        self.drafts
            .iter()
            .rev()
            .find(|draft| !draft.content.trim().is_empty())
            .map(|draft| draft.content.clone())
            .unwrap_or_default()
    }
}

/// Drives one turn.
pub struct AgentLoop<'a> {
    provider: &'a dyn Provider,
    registry: &'a ToolRegistry,
    host: &'a mut dyn ToolHost,
}

impl<'a> AgentLoop<'a> {
    /// Build a loop over a provider, the built-in tool registry and a host.
    pub fn new(
        provider: &'a dyn Provider,
        registry: &'a ToolRegistry,
        host: &'a mut dyn ToolHost,
    ) -> Self {
        Self {
            provider,
            registry,
            host,
        }
    }

    /// Tools this session may even be offered.
    ///
    /// A tool the policy denies is not advertised at all: it is not enough to
    /// refuse the call, the model should not spend a round discovering that.
    pub fn offered_tools(&self, plan: &TurnPlan) -> Vec<ToolDefinition> {
        self.registry
            .iter()
            .filter(|spec| {
                plan.permissions.decide(spec, plan.mode, plan.origin) != PolicyDecision::Deny
            })
            .map(ToolDefinition::from)
            .collect()
    }

    /// Run the turn, streaming normalized events as they happen.
    pub fn run(
        &mut self,
        plan: &TurnPlan,
        cancel: &Cancellation,
        events: &mut dyn FnMut(AiEventPayload),
    ) -> AgentOutcome {
        let mut outcome = AgentOutcome {
            turn_id: plan.turn_id.clone(),
            drafts: Vec::new(),
            tool_results: Vec::new(),
            usage: Usage::default(),
            stop_reason: StopReason::EndTurn,
            error: None,
            rounds: 0,
            context: plan.context.clone(),
        };

        events(AiEventPayload::TurnStarted {
            turn_id: plan.turn_id.clone(),
            seq: plan.seq,
        });
        events(AiEventPayload::ContextReady {
            sha256: plan.context.context_sha256.clone(),
            tokens: plan.context.tokens_estimate,
            spec_version: plan.context.spec_version.clone(),
        });

        let mut messages = self.initial_messages(plan);
        let tools = self.offered_tools(plan);
        let mut failures: HashMap<String, u32> = HashMap::new();

        while outcome.rounds < plan.limits.max_rounds.max(1) {
            if cancel.is_cancelled() {
                outcome.stop_reason = StopReason::Cancelled;
                break;
            }
            outcome.rounds += 1;

            let request = CompletionRequest::new(plan.model.model.clone(), messages.clone())
                .with_tools(tools.clone())
                .with_temperature(plan.temperature);
            let request = match plan.max_tokens {
                Some(max_tokens) => request.with_max_tokens(max_tokens),
                None => request,
            };

            let response = {
                let mut sink = |event: ProviderEvent| match event {
                    ProviderEvent::ContentDelta(text) => {
                        events(AiEventPayload::ContentDelta { text });
                    }
                    ProviderEvent::ReasoningDelta(text) => {
                        events(AiEventPayload::ReasoningDelta { text });
                    }
                    // Tool deltas are reported once assembled, so the UI shows
                    // one card per call rather than a growing fragment.
                    _ => {}
                };
                self.provider.complete(&request, cancel, &mut sink)
            };

            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    outcome.stop_reason = if error.code == crate::provider::error_code::CANCELLED {
                        StopReason::Cancelled
                    } else {
                        StopReason::Error
                    };
                    events(AiEventPayload::Error {
                        error: error.clone(),
                    });
                    outcome.error = Some(error.clone());
                    outcome.drafts.push(TurnDraft {
                        content: String::new(),
                        reasoning: None,
                        tool_calls: Vec::new(),
                        tool_results: Vec::new(),
                        usage: Usage::default(),
                        stop_reason: outcome.stop_reason,
                        error: Some(error),
                    });
                    break;
                }
            };

            outcome.usage = add_usage(outcome.usage, response.usage);
            events(AiEventPayload::Usage {
                usage: response.usage,
            });

            let mut draft = TurnDraft {
                content: response.content.clone(),
                reasoning: response.reasoning.clone(),
                tool_calls: response.tool_calls.clone(),
                tool_results: Vec::new(),
                usage: response.usage,
                stop_reason: response.stop_reason,
                error: None,
            };

            if response.tool_calls.is_empty() {
                outcome.stop_reason = response.stop_reason;
                outcome.drafts.push(draft);
                break;
            }

            messages.push(ChatMessage::assistant_tools(
                non_empty(&response.content),
                response.tool_calls.clone(),
            ));

            let mut stop_after_tools = None;
            for call in &response.tool_calls {
                // Cancelling stops every remaining call: a turn that is being
                // stopped must not write to a board.
                if cancel.is_cancelled() {
                    stop_after_tools = Some(StopReason::Cancelled);
                    break;
                }

                let result = self.execute(plan, call, &mut failures, events);
                events(AiEventPayload::ToolResult {
                    id: result.call_id.clone(),
                    ok: result.ok,
                    code: result.code.clone(),
                    summary: result.summary.clone(),
                });
                if !result.ok && result.code == result_code::APPROVAL_REQUIRED {
                    let approval_id = result
                        .data
                        .as_ref()
                        .and_then(|data| data.get("approval_id"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| call.id.clone());
                    events(AiEventPayload::ApprovalRequired {
                        approval_id,
                        changeset_summary: result.summary.clone(),
                    });
                }
                if let (Some(board_id), Some(revision)) = (
                    result.data.as_ref().and_then(|data| {
                        data.get("board_id").and_then(Value::as_str).map(str::to_string)
                    }),
                    result.revision,
                ) {
                    events(AiEventPayload::BoardAction {
                        board_id,
                        revision,
                        action: call.name.clone(),
                    });
                }

                messages.push(ChatMessage::tool_result(
                    call.id.clone(),
                    tool_payload(&result),
                ));
                draft.tool_results.push(result.clone());
                outcome.tool_results.push(result);

                if let Some((code, message)) = self.exhausted(&failures, call, plan) {
                    events(AiEventPayload::Error {
                        error: ErrorInfo {
                            code: code.to_string(),
                            message: message.clone(),
                            retryable: false,
                            provider_code: None,
                        },
                    });
                    outcome.error = Some(ErrorInfo {
                        code: code.to_string(),
                        message,
                        retryable: false,
                        provider_code: None,
                    });
                    stop_after_tools = Some(StopReason::Error);
                    break;
                }
            }

            outcome.drafts.push(draft);
            if let Some(stop) = stop_after_tools {
                outcome.stop_reason = stop;
                break;
            }
            if outcome.rounds >= plan.limits.max_rounds.max(1) {
                let error = ErrorInfo {
                    code: loop_code::ROUNDS_EXHAUSTED.to_string(),
                    message: format!(
                        "the assistant used all {} tool rounds without finishing",
                        plan.limits.max_rounds
                    ),
                    retryable: true,
                    provider_code: None,
                };
                events(AiEventPayload::Error {
                    error: error.clone(),
                });
                outcome.error = Some(error);
                outcome.stop_reason = StopReason::Error;
            }
        }

        events(AiEventPayload::TurnDone {
            turn_id: plan.turn_id.clone(),
            stop_reason: outcome.stop_reason,
        });
        outcome
    }

    fn initial_messages(&self, plan: &TurnPlan) -> Vec<ChatMessage> {
        let mut messages = Vec::with_capacity(plan.history.len() + 2);
        // The compiled context is the one system message; any other would
        // duplicate it and drift from the recorded hash.
        if !plan.context.system.trim().is_empty() {
            messages.push(ChatMessage::system(plan.context.system.clone()));
        }
        messages.extend(
            plan.history
                .iter()
                .filter(|message| message.role != TurnRole::System)
                .cloned(),
        );
        messages.push(ChatMessage::user(plan.user_message.clone()));
        messages
    }

    /// Apply the policy, then run the call if it is allowed.
    fn execute(
        &mut self,
        plan: &TurnPlan,
        call: &ToolCall,
        failures: &mut HashMap<String, u32>,
        events: &mut dyn FnMut(AiEventPayload),
    ) -> ToolResult {
        let Some(spec) = self.registry.get(&call.name) else {
            count_failure(failures, &call.name);
            let result = refused(
                call,
                result_code::NOT_FOUND,
                format!("there is no tool called {}", call.name),
            );
            events(AiEventPayload::ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
            return result;
        };

        if let Err(error) = crate::tools::validate_arguments(spec, &call.arguments) {
            count_failure(failures, &call.name);
            events(AiEventPayload::ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
            return refused(call, result_code::INVALID_ARGUMENTS, error.to_string());
        }

        events(AiEventPayload::ToolCall {
            id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
        });

        match plan.permissions.decide(spec, plan.mode, plan.origin) {
            PolicyDecision::Deny => {
                count_failure(failures, &call.name);
                refused(
                    call,
                    result_code::DENIED,
                    format!("{} is not available in this mode", call.name),
                )
            }
            PolicyDecision::RequiresApproval => {
                // Not a failure: the model asked for something a person has to
                // approve, and the turn continues around it.
                let summary = format!(
                    "{} changes stored content and needs approval before it runs",
                    call.name
                );
                let mut result = refused(call, result_code::APPROVAL_REQUIRED, summary.clone());
                if let Some(approval_id) = self.host.request_approval(call, summary) {
                    result.data = Some(serde_json::json!({"approval_id": approval_id}));
                }
                result
            }
            PolicyDecision::Allow => {
                let result = self.host.call(call);
                if !result.ok {
                    count_failure(failures, &call.name);
                } else {
                    failures.remove(&call.name);
                }
                result
            }
        }
    }

    /// Whether a tool has failed so often that the loop should stop it.
    fn exhausted(
        &self,
        failures: &HashMap<String, u32>,
        call: &ToolCall,
        plan: &TurnPlan,
    ) -> Option<(&'static str, String)> {
        let count = failures.get(&call.name).copied().unwrap_or(0);
        if count > plan.limits.max_tool_retries {
            return Some((
                loop_code::TOOL_RETRIES_EXHAUSTED,
                format!(
                    "{} failed {count} times; the turn stopped rather than repeating it",
                    call.name
                ),
            ));
        }
        None
    }
}

fn count_failure(failures: &mut HashMap<String, u32>, tool: &str) {
    *failures.entry(tool.to_string()).or_insert(0) += 1;
}

fn non_empty(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_string())
}

/// The JSON the model reads for a tool result.
fn tool_payload(result: &ToolResult) -> String {
    match serde_json::to_string(result) {
        Ok(json) => json,
        Err(_) => format!(
            "{{\"ok\":{},\"code\":\"{}\",\"summary\":\"{}\"}}",
            result.ok, result.code, result.summary
        ),
    }
}

fn refused(call: &ToolCall, code: &str, summary: String) -> ToolResult {
    ToolResult {
        call_id: call.id.clone(),
        tool: call.name.clone(),
        ok: false,
        code: code.to_string(),
        summary,
        data: None,
        revision: None,
        audit_id: None,
        error: None,
    }
}

fn add_usage(total: Usage, next: Usage) -> Usage {
    Usage {
        tokens_in: total.tokens_in.saturating_add(next.tokens_in),
        tokens_out: total.tokens_out.saturating_add(next.tokens_out),
        latency_ms: total.latency_ms.saturating_add(next.latency_ms),
        cost_estimate: match (total.cost_estimate, next.cost_estimate) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        },
    }
}
