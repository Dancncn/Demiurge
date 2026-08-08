use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    BudgetLimited,
    UsageLimited,
    MaxTurns,
    Complete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GoalState {
    pub objective: String,
    pub status: GoalStatus,
    pub token_budget: Option<usize>,
    pub tokens_used: usize,
    pub start_time: u64,
    pub paused_at: Option<u64>,
    pub accumulated_active_ms: u64,
    pub blocked_attempts: usize,
    pub last_block_reason: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub turns_executed: usize,
    #[serde(default)]
    pub budget_limit_notified: bool,
}
