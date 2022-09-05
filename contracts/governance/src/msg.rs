use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::{ProposalStatus, VoteOption};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub bjmes_token_addr: String,
    pub proposal_required_deposit: Uint128,
    // Required percentage for a proposal to pass, e.g. 51
    pub proposal_required_percentage: u64,
    // Epoch when the 1st posting period starts, e.g. 1660000000
    pub period_start_epoch: u64,
    // Length in seconds of the posting period, e.g.  606864 for ~ 1 Week (year/52)
    pub posting_period_length: u64,
    // Length in seconds of the posting period, e.g.  606864 for ~ 1 Week (year/52)
    pub voting_period_length: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receive a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    Vote {
        id: u64,
        vote: VoteOption,
    },
    Conclude {
        id: u64,
    }, // ImprovementProposal {},
       // ConfigChangeProposal {},

       // RequestFeature { feature: Feature },
       // RemoveFeature { feature: Feature },

       // RequestCoreSlot { core_slot: CoreSlot },

       // RemoveCoreSlot { core_slot: CoreSlot },
       // BurnArtistNft {},
       // BurnArtNft {}
}

/// This structure stores data for a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    TextProposal { title: String, description: String },
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    ArtistCurator {},
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoreSlot {
    Brand {},
    Marketing {},
    Creative {},
    BizDev {},
    CoreTech {},
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    PeriodInfo {},
    Proposal {
        id: u64,
    },
    Proposals {
        start: Option<u64>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalPeriod {
    Posting,
    Voting,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PeriodInfoResponse {
    pub current_block: u64,
    pub current_period: ProposalPeriod,
    pub current_time_in_cycle: u64,
    pub current_posting_start: u64,
    pub current_voting_start: u64,
    pub current_voting_end: u64,
    pub next_posting_start: u64,
    pub next_voting_start: u64,
    pub posting_period_length: u64,
    pub voting_period_length: u64,
    pub cycle_length: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ProposalResponse {
    pub id: u64,
    pub dao: Addr,
    pub title: String,
    pub description: String,
    pub coins_yes: Uint128,
    pub coins_no: Uint128,
    pub yes_voters: Vec<Addr>,
    pub no_voters: Vec<Addr>,
    pub deposit_amount: Uint128,
    pub start_block: u64,
    pub posting_start: u64,
    pub voting_start: u64,
    pub voting_end: u64,
    pub concluded: bool,
    pub status: ProposalStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ProposalsResponse {
    pub proposal_count: u64,
    pub proposals: Vec<ProposalResponse>,
}
