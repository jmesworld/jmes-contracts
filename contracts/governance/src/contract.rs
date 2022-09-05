use crate::error::ContractError;
use crate::msg::{Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Config, CONFIG, PROPOSAL_COUNT};
use bjmes_token::msg::QueryMsg as BjmesQueryMsg;
use cosmwasm_std::{
    from_binary, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;

// version info for migration info
const CONTRACT_NAME: &str = "governance";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Default pagination constants
const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config {
        bjmes_token_addr: deps.api.addr_validate(&msg.bjmes_token_addr)?,
        proposal_required_deposit: msg.proposal_required_deposit,
        proposal_required_percentage: msg.proposal_required_percentage, // 51
        period_start_epoch: msg.period_start_epoch,                     // 1660000000,
        posting_period_length: msg.posting_period_length,               // 300000,
        voting_period_length: msg.voting_period_length,                 // 606864,
    };

    CONFIG.save(deps.storage, &config)?;

    PROPOSAL_COUNT.save(deps.storage, &(0 as u64))?;
    Ok(Response::new())
}

pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use QueryMsg::*;

    match msg {
        Config {} => to_binary(&CONFIG.load(deps.storage)?),
        PeriodInfo {} => to_binary(&query::period_info(deps, env)?),
        Proposal { id } => to_binary(&query::proposal(deps, env, id)?),
        Proposals { start, limit } => to_binary(&query::proposals(deps, env, start, limit)?),
    }
}

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    use ExecuteMsg::*;

    match msg {
        Receive(cw20_msg) => exec::receive_cw20(deps, env, info, cw20_msg),
        Vote { id, vote } => exec::vote(deps, env, info, id, vote),
        Conclude { id } => exec::conclude(deps, env, id),
    }
}

mod exec {
    use crate::msg::ProposalPeriod;
    use crate::state::VoteOption::*;
    use crate::{contract::query::period_info, state::VoteOption};
    use cosmwasm_std::{Addr, CosmosMsg, Uint128, WasmMsg};
    use cw20::{BalanceResponse, Cw20ExecuteMsg};

    use super::*;
    use crate::state::{Proposal, ProposalStatus, PROPOSALS};

    pub fn receive_cw20(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        cw20_msg: Cw20ReceiveMsg,
    ) -> Result<Response, ContractError> {
        match from_binary(&cw20_msg.msg)? {
            Cw20HookMsg::TextProposal { title, description } => {
                // let sender = addr_validate_to_lower(deps.api, &cw20_msg.sender)?;
                let sender = deps.api.addr_validate(&cw20_msg.sender)?;
                text_proposal(deps, info, env, sender, cw20_msg.amount, title, description)
            }
        }
    }
    pub fn text_proposal(
        deps: DepsMut,
        info: MessageInfo,
        env: Env,
        sender: Addr,
        deposit_amount: Uint128,
        title: String,
        description: String,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;

        let period_info = period_info(deps.as_ref(), env.clone())?;

        if period_info.current_period != ProposalPeriod::Posting {
            return Err(ContractError::NotPostingPeriod {});
        }

        if info.sender != config.bjmes_token_addr {
            return Err(ContractError::Unauthorized {});
        }

        if deposit_amount < Uint128::from(config.proposal_required_deposit) {
            return Err(ContractError::InsufficientDeposit {});
        }

        let id = Proposal::next_id(deps.storage)?;
        let proposal = Proposal {
            id,
            dao: sender,
            title,
            description,
            coins_no: Uint128::zero(),
            coins_yes: Uint128::zero(),
            yes_voters: Vec::new(),
            no_voters: Vec::new(),
            deposit_amount,
            start_block: env.block.height, // used for voting coin lookup
            posting_start: period_info.current_posting_start,
            voting_start: period_info.current_voting_start,
            voting_end: period_info.current_voting_end,
            concluded: false,
        };

        PROPOSALS.save(deps.storage, id, &proposal)?;

        Ok(Response::new())
    }

    pub fn vote(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        id: u64,
        vote: VoteOption,
    ) -> Result<Response, ContractError> {
        {
            let config = CONFIG.load(deps.storage)?;

            let period_info = period_info(deps.as_ref(), env.clone())?;

            if period_info.current_period != ProposalPeriod::Voting {
                return Err(ContractError::NotVotingPeriod {});
            }

            let mut proposal = PROPOSALS.load(deps.storage, id)?;

            if proposal.yes_voters.contains(&info.sender)
                || proposal.no_voters.contains(&info.sender)
            {
                return Err(ContractError::UserAlreadyVoted {});
            }

            let bjmes_amount: BalanceResponse = deps.querier.query_wasm_smart(
                config.bjmes_token_addr,
                &BjmesQueryMsg::Balance {
                    address: info.sender.to_string(),
                    // block: proposal.start_block,
                },
            )?;

            let vote_coins = bjmes_amount.balance;

            if vote_coins.is_zero() {
                return Err(ContractError::NoVoteCoins {});
            }

            match vote {
                Yes => {
                    proposal.coins_yes = proposal.coins_yes.checked_add(vote_coins)?;
                    proposal.yes_voters.push(info.sender.clone());
                }
                No => {
                    proposal.coins_no = proposal.coins_no.checked_add(vote_coins)?;
                    proposal.no_voters.push(info.sender.clone());
                }
            };

            PROPOSALS.save(deps.storage, id, &proposal)?;

            Ok(Response::new())
        }
    }

    // Refund deposit_amount and execute msgs
    pub fn conclude(deps: DepsMut, env: Env, id: u64) -> Result<Response, ContractError> {
        let mut proposal = PROPOSALS.load(deps.storage, id)?;
        let config = CONFIG.load(deps.storage)?;

        if env.block.time.seconds() <= proposal.voting_end {
            return Err(ContractError::VotingPeriodNotEnded {});
        }

        if proposal.status(env, config.proposal_required_percentage) == ProposalStatus::Success {
            // TODO attach execute msgs
        }

        proposal.concluded = true;

        PROPOSALS.save(deps.storage, id, &proposal)?;

        let refund_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.bjmes_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: proposal.dao.to_string(),
                amount: proposal.deposit_amount,
            })?,
            funds: vec![],
        });

        Ok(Response::new().add_messages(vec![refund_msg]))
    }
}

mod query {
    use std::ops::Sub;

    use cosmwasm_std::Order;
    use cw_storage_plus::Bound;

    use crate::msg::{PeriodInfoResponse, ProposalPeriod, ProposalResponse, ProposalsResponse};
    use crate::state::{PROPOSALS, PROPOSAL_COUNT};

    use super::*;

    pub fn period_info(deps: Deps, env: Env) -> StdResult<PeriodInfoResponse> {
        let config = CONFIG.load(deps.storage)?;

        let now = env.block.time.seconds();

        let time_delta = now.sub(config.period_start_epoch);

        let full_cycle = config
            .posting_period_length
            .checked_add(config.voting_period_length)
            .unwrap();

        let time_in_cycle = time_delta % full_cycle;

        let current_period = if time_in_cycle > config.posting_period_length {
            ProposalPeriod::Voting
        } else {
            ProposalPeriod::Posting
        };

        let current_posting_start = now - time_in_cycle;
        let current_voting_start = current_posting_start + config.posting_period_length;
        let current_voting_end = current_voting_start + config.voting_period_length;

        let next_posting_start = current_posting_start + full_cycle;
        let next_voting_start = current_voting_start + full_cycle;

        Ok(PeriodInfoResponse {
            current_block: env.block.height,
            current_period,
            current_time_in_cycle: time_in_cycle,
            current_posting_start,
            current_voting_start,
            current_voting_end,
            next_posting_start,
            next_voting_start,
            posting_period_length: config.posting_period_length,
            voting_period_length: config.voting_period_length,
            cycle_length: config.posting_period_length + config.voting_period_length,
        })
    }

    pub fn proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse> {
        let proposal = PROPOSALS.load(deps.storage, id)?;
        let config = CONFIG.load(deps.storage)?;

        Ok(ProposalResponse {
            id: proposal.id,
            dao: proposal.dao.clone(),
            title: proposal.title.clone(),
            description: proposal.description.clone(),
            coins_yes: proposal.coins_yes,
            coins_no: proposal.coins_no,
            yes_voters: proposal.yes_voters.clone(),
            no_voters: proposal.no_voters.clone(),
            deposit_amount: proposal.deposit_amount,
            start_block: proposal.start_block,
            posting_start: proposal.posting_start,
            voting_start: proposal.voting_start,
            voting_end: proposal.voting_end,
            concluded: proposal.concluded,
            status: proposal.status(env.clone(), config.proposal_required_percentage),
        })
    }

    pub fn proposals(
        deps: Deps,
        env: Env,
        start: Option<u64>,
        limit: Option<u32>,
    ) -> StdResult<ProposalsResponse> {
        let proposal_count = PROPOSAL_COUNT.load(deps.storage)?;
        let config = CONFIG.load(deps.storage)?;

        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start = start.map(|start| Bound::inclusive(start));

        let proposals = PROPOSALS
            .range(deps.storage, start, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                let (_, proposal) = item?;
                Ok(ProposalResponse {
                    id: proposal.id,
                    dao: proposal.dao.clone(),
                    title: proposal.title.clone(),
                    description: proposal.description.clone(),
                    coins_yes: proposal.coins_yes,
                    coins_no: proposal.coins_no,
                    yes_voters: proposal.yes_voters.clone(),
                    no_voters: proposal.no_voters.clone(),
                    deposit_amount: proposal.deposit_amount,
                    start_block: proposal.start_block,
                    posting_start: proposal.posting_start,
                    voting_start: proposal.voting_start,
                    voting_end: proposal.voting_end,
                    concluded: proposal.concluded,
                    status: proposal.status(env.clone(), config.proposal_required_percentage),
                })
            })
            .collect::<StdResult<Vec<_>>>()?;

        Ok(ProposalsResponse {
            proposal_count,
            proposals,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::testing::MockApi;
    use cosmwasm_std::testing::MockStorage;
    use cosmwasm_std::Addr;
    use cosmwasm_std::Timestamp;
    use cosmwasm_std::Uint128;
    use cw20::BalanceResponse;
    use cw20::Cw20ExecuteMsg;
    use cw_multi_test::AppBuilder;
    use cw_multi_test::BankKeeper;
    use cw_multi_test::{ContractWrapper, Executor};

    use bjmes_token::contract::execute as bjmes_execute;
    use bjmes_token::contract::instantiate as bjmes_instantiate;
    use bjmes_token::contract::query as bjmes_query;
    use bjmes_token::msg::ExecuteMsg as BjmesExecuteMsg;
    use bjmes_token::msg::InstantiateMsg as BjmesInstantiateMsg;
    use bjmes_token::msg::QueryMsg as BjmesQueryMsg;

    use crate::state::ProposalStatus;
    use crate::state::VoteOption::{No, Yes};

    use crate::msg::*;

    use super::*;

    const PROPOSAL_REQUIRED_DEPOSIT: u128 = 1000;

    #[test]
    fn text_proposal() {
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1660000010);
        let api = MockApi::default();
        let bank = BankKeeper::new();
        let storage = MockStorage::new();

        let mut app = AppBuilder::new()
            .with_api(api)
            .with_block(env.block)
            .with_bank(bank)
            .with_storage(storage)
            .build(|_, _, _| {});

        let owner = Addr::unchecked("owner");
        let user1 = Addr::unchecked("user1");
        let user2 = Addr::unchecked("user2");

        // Instantiate bonded JMES cw20 contract
        let bjmes_code = ContractWrapper::new(bjmes_execute, bjmes_instantiate, bjmes_query);
        let bjmes_code_id = app.store_code(Box::new(bjmes_code));

        let bjmes_instance = app
            .instantiate_contract(
                bjmes_code_id,
                owner,
                &BjmesInstantiateMsg {
                    name: "bonded JMES".to_string(),
                    symbol: "bjmes".to_string(),
                    decimals: 10,
                    initial_balances: vec![],
                    marketing: None,
                    mint: None,
                },
                &[],
                "bonded JMES Contract",
                None,
            )
            .unwrap();

        // Mint bJMES token
        let _mint_resp = app
            .execute_contract(
                user1.clone(),
                bjmes_instance.clone(),
                &BjmesExecuteMsg::Mint {
                    recipient: user1.clone().to_string(),
                    amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT * 2),
                },
                &[],
            )
            .unwrap();

        let _mint_resp = app
            .execute_contract(
                user2.clone(),
                bjmes_instance.clone(),
                &BjmesExecuteMsg::Mint {
                    recipient: user2.clone().to_string(),
                    amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT - 50u128),
                },
                &[],
            )
            .unwrap();

        // Query bJMES token balance
        let msg = BjmesQueryMsg::Balance {
            address: user1.clone().to_string(),
        };
        let resp: StdResult<BalanceResponse> =
            app.wrap().query_wasm_smart(bjmes_instance.clone(), &msg);

        assert_eq!(
            resp.unwrap().balance,
            Uint128::from(PROPOSAL_REQUIRED_DEPOSIT * 2)
        );

        // Instantiate governance contract
        let governance_code = ContractWrapper::new(execute, instantiate, query);
        let governance_code_id = app.store_code(Box::new(governance_code));

        let governance_instance = app
            .instantiate_contract(
                governance_code_id,
                Addr::unchecked("owner"),
                &InstantiateMsg {
                    bjmes_token_addr: bjmes_instance.clone().to_string(),
                    proposal_required_deposit: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
                    proposal_required_percentage: 51,
                    period_start_epoch: 1660000000,
                    posting_period_length: 300000,
                    voting_period_length: 606864,
                },
                &[],
                "Governance Contract",
                None,
            )
            .unwrap();

        // Query empty proposals
        let resp: ProposalsResponse = app
            .wrap()
            .query_wasm_smart(
                governance_instance.clone(),
                &QueryMsg::Proposals {
                    start: None,
                    limit: None,
                },
            )
            .unwrap();

        assert_eq!(
            resp,
            ProposalsResponse {
                proposal_count: 0,
                proposals: vec![]
            }
        );

        // Query contract config
        let config: Config = app
            .wrap()
            .query_wasm_smart(governance_instance.clone(), &QueryMsg::Config {})
            .unwrap();

        assert_eq!(config.bjmes_token_addr, bjmes_instance);
        assert_eq!(
            config.proposal_required_deposit,
            Uint128::from(config.proposal_required_deposit)
        );

        // Query PeriodInfo: Posting
        let res: PeriodInfoResponse = app
            .wrap()
            .query_wasm_smart(governance_instance.clone(), &QueryMsg::PeriodInfo {})
            .unwrap();

        assert_eq!(res.current_period, ProposalPeriod::Posting);
        assert_eq!(res.current_time_in_cycle, 10);

        // Skip period from Posting to Voting
        app.update_block(|mut block| {
            block.time = block.time.plus_seconds(config.posting_period_length);
            block.height += config.posting_period_length / 5;
        });

        // Query PeriodInfo: Voting
        let res: PeriodInfoResponse = app
            .wrap()
            .query_wasm_smart(governance_instance.clone(), &QueryMsg::PeriodInfo {})
            .unwrap();

        assert_eq!(res.current_period, ProposalPeriod::Voting);
        assert_eq!(res.current_time_in_cycle, 10 + config.posting_period_length);

        // Test proposal in voting period
        let text_proposal_msg = Cw20ExecuteMsg::Send {
            contract: governance_instance.to_string(),
            msg: to_binary(&Cw20HookMsg::TextProposal {
                title: String::from("Text"),
                description: String::from("Proposal"),
            })
            .unwrap(),
            amount: Uint128::from(config.proposal_required_deposit),
        };

        let err = app
            .execute_contract(
                user1.clone(),
                bjmes_instance.clone(),
                &text_proposal_msg,
                &[],
            )
            .unwrap_err();

        assert_eq!(err.root_cause().to_string(), "NotPostingPeriod");

        // Skip period from Voting to Posting
        app.update_block(|mut block| {
            block.time = block.time.plus_seconds(config.voting_period_length);
            block.height += config.posting_period_length / 5;
        });

        // Test proposal with insufficient deposit amount
        let text_proposal_msg = Cw20ExecuteMsg::Send {
            contract: governance_instance.to_string(),
            msg: to_binary(&Cw20HookMsg::TextProposal {
                title: String::from("Text"),
                description: String::from("Proposal"),
            })
            .unwrap(),
            amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT - 1u128),
        };

        let err = app
            .execute_contract(
                user1.clone(),
                bjmes_instance.clone(),
                &text_proposal_msg,
                &[],
            )
            .unwrap_err();

        assert_eq!(err.root_cause().to_string(), "Insufficient token deposit!");

        // Test valid proposal submission
        let text_proposal_msg = Cw20ExecuteMsg::Send {
            contract: governance_instance.to_string(),
            msg: to_binary(&Cw20HookMsg::TextProposal {
                title: String::from("Text"),
                description: String::from("Proposal"),
            })
            .unwrap(),
            amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
        };

        let _resp = app
            .execute_contract(
                user1.clone(),
                bjmes_instance.clone(),
                &text_proposal_msg,
                &[],
            )
            .unwrap();

        let resp: ProposalResponse = app
            .wrap()
            .query_wasm_smart(governance_instance.clone(), &QueryMsg::Proposal { id: 1 })
            .unwrap();
        assert_eq!(
            resp,
            ProposalResponse {
                id: 1,
                dao: user1.clone(),
                title: "Text".to_string(),
                description: "Proposal".to_string(),
                coins_yes: Uint128::zero(),
                coins_no: Uint128::zero(),
                yes_voters: vec![],
                no_voters: vec![],
                deposit_amount: Uint128::from(1000u128),
                start_block: 132345,
                posting_start: 1660906864,
                voting_start: 1661206864,
                voting_end: 1661813728,
                concluded: false,
                status: ProposalStatus::Posted
            }
        );

        let resp: ProposalsResponse = app
            .wrap()
            .query_wasm_smart(
                governance_instance.clone(),
                &QueryMsg::Proposals {
                    start: None,
                    limit: None,
                },
            )
            .unwrap();

        assert_eq!(
            resp,
            ProposalsResponse {
                proposal_count: 1,
                proposals: vec![ProposalResponse {
                    id: 1,
                    dao: user1.clone(),
                    title: "Text".to_string(),
                    description: "Proposal".to_string(),
                    coins_yes: Uint128::zero(),
                    coins_no: Uint128::zero(),
                    yes_voters: vec![],
                    no_voters: vec![],
                    deposit_amount: Uint128::from(1000u128),
                    start_block: 132345,
                    posting_start: 1660906864,
                    voting_start: 1661206864,
                    voting_end: 1661813728,
                    concluded: false,
                    status: ProposalStatus::Posted
                }]
            }
        );

        // Query bJMES token balance after proposal submission
        let msg = BjmesQueryMsg::Balance {
            address: user1.clone().to_string(),
        };
        let resp: StdResult<BalanceResponse> =
            app.wrap().query_wasm_smart(bjmes_instance.clone(), &msg);

        assert_eq!(
            resp.unwrap().balance,
            Uint128::from(config.proposal_required_deposit)
        );

        // TODO test vote with no coins

        // Test proposal vote in posting period
        let vote_msg = ExecuteMsg::Vote { id: 1, vote: Yes };

        let err = app
            .execute_contract(user1.clone(), governance_instance.clone(), &vote_msg, &[])
            .unwrap_err();

        assert_eq!(err.root_cause().to_string(), "NotVotingPeriod");

        // Skip period from Posting to Voting
        app.update_block(|mut block| {
            block.time = block.time.plus_seconds(config.posting_period_length);
            block.height += config.posting_period_length / 5;
        });

        // Query PeriodInfo: Voting
        let res: PeriodInfoResponse = app
            .wrap()
            .query_wasm_smart(governance_instance.clone(), &QueryMsg::PeriodInfo {})
            .unwrap();

        assert_eq!(res.current_period, ProposalPeriod::Voting);
        assert_eq!(res.current_time_in_cycle, 10 + config.posting_period_length);

        // Test proposal yes vote
        let vote_msg = ExecuteMsg::Vote { id: 1, vote: Yes };

        let _resp = app
            .execute_contract(user1.clone(), governance_instance.clone(), &vote_msg, &[])
            .unwrap();

        let resp: ProposalResponse = app
            .wrap()
            .query_wasm_smart(governance_instance.clone(), &QueryMsg::Proposal { id: 1 })
            .unwrap();

        assert_eq!(
            resp,
            ProposalResponse {
                id: 1,
                dao: user1.clone(),
                title: "Text".to_string(),
                description: "Proposal".to_string(),
                coins_yes: Uint128::from(1000u128),
                coins_no: Uint128::zero(),
                yes_voters: vec![user1.clone()],
                no_voters: vec![],
                deposit_amount: Uint128::from(1000u128),
                start_block: 132345,
                posting_start: 1660906864,
                voting_start: 1661206864,
                voting_end: 1661813728,
                concluded: false,
                status: ProposalStatus::Voting
            }
        );

        // Test proposal yes vote a second time
        let vote_msg = ExecuteMsg::Vote { id: 1, vote: Yes };

        let err = app
            .execute_contract(user1.clone(), governance_instance.clone(), &vote_msg, &[])
            .unwrap_err();

        assert_eq!(err.root_cause().to_string(), "User already voted!");

        // Test proposal no vote
        let vote_msg = ExecuteMsg::Vote { id: 1, vote: No };

        let _resp = app
            .execute_contract(user2.clone(), governance_instance.clone(), &vote_msg, &[])
            .unwrap();

        let resp: ProposalResponse = app
            .wrap()
            .query_wasm_smart(governance_instance.clone(), &QueryMsg::Proposal { id: 1 })
            .unwrap();

        assert_eq!(
            resp,
            ProposalResponse {
                id: 1,
                dao: user1.clone(),
                title: "Text".to_string(),
                description: "Proposal".to_string(),
                coins_yes: Uint128::from(1000u128),
                coins_no: Uint128::from(950u128),
                yes_voters: vec![user1.clone()],
                no_voters: vec![user2.clone()],
                deposit_amount: Uint128::from(1000u128),
                start_block: 132345,
                posting_start: 1660906864,
                voting_start: 1661206864,
                voting_end: 1661813728,
                concluded: false,
                status: ProposalStatus::Voting
            }
        );

        // Test proposal no vote a second time
        let vote_msg = ExecuteMsg::Vote { id: 1, vote: No };

        let err = app
            .execute_contract(user2.clone(), governance_instance.clone(), &vote_msg, &[])
            .unwrap_err();

        assert_eq!(err.root_cause().to_string(), "User already voted!");

        // Test conclude proposal still in voting period
        let msg = ExecuteMsg::Conclude { id: 1 };

        let err = app
            .execute_contract(user1.clone(), governance_instance.clone(), &msg, &[])
            .unwrap_err();

        assert_eq!(err.root_cause().to_string(), "VotingPeriodNotEnded");

        // Skip period from Voting to Posting
        app.update_block(|mut block| {
            block.time = block.time.plus_seconds(config.voting_period_length);
            block.height += config.posting_period_length / 5;
        });

        // Test conclude passing proposal
        let msg = ExecuteMsg::Conclude { id: 1 };

        let _resp = app
            .execute_contract(user1.clone(), governance_instance.clone(), &msg, &[])
            .unwrap();

        let resp_concluded: ProposalResponse = app
            .wrap()
            .query_wasm_smart(governance_instance.clone(), &QueryMsg::Proposal { id: 1 })
            .unwrap();
        println!("\n\n_resp {:?}", _resp);
        assert_eq!(
            resp_concluded,
            ProposalResponse {
                id: 1,
                dao: user1.clone(),
                title: "Text".to_string(),
                description: "Proposal".to_string(),
                coins_yes: Uint128::from(1000u128),
                coins_no: Uint128::from(950u128),
                yes_voters: vec![user1.clone()],
                no_voters: vec![user2.clone()],
                deposit_amount: Uint128::from(1000u128),
                start_block: 132345,
                posting_start: 1660906864,
                voting_start: 1661206864,
                voting_end: 1661813728,
                concluded: true,
                status: ProposalStatus::SuccessConcluded
            }
        );

        // TODO test expiredconcluded proposal

        // Query bJMES token balance after proposal conclusion
        let msg = BjmesQueryMsg::Balance {
            address: user1.clone().to_string(),
        };
        let resp: StdResult<BalanceResponse> =
            app.wrap().query_wasm_smart(bjmes_instance.clone(), &msg);

        assert_eq!(
            resp.unwrap().balance,
            Uint128::from(PROPOSAL_REQUIRED_DEPOSIT * 2)
        );

        // TODO conclude expired proposal
    }
}
