/**
* This file was automatically generated by @jmes-cosmwasm/ts-codegen@0.14.2.
* DO NOT MODIFY IT BY HAND. Instead, modify the source JSONSchema file,
* and run the @jmes-cosmwasm/ts-codegen generate command to regenerate this file.
*/

import { LCDClient, Coins, MnemonicKey, MsgExecuteContract, WaitTxBroadcastResult } from "@terra-money/terra.js";
import { ExecuteResult } from "@cosmjs/cosmwasm-stargate";
import { StdFee } from "@cosmjs/amino";
import { Addr, DaosResponse, ExecuteMsg, Duration, Threshold, Decimal, DaoInstantiateMsg, Voter, IdType, GetIdentityByNameResponse, Identity, GetIdentityByOwnerResponse, InstantiateMsg, QueryMsg, Ordering } from "./Identityservice.types";
export interface IdentityserviceReadOnlyInterface {
  contractAddress: string;
  getIdentityByOwner: ({
    owner
  }: {
    owner: string;
  }) => Promise<GetIdentityByOwnerResponse>;
  getIdentityByName: ({
    name
  }: {
    name: string;
  }) => Promise<GetIdentityByNameResponse>;
  daos: ({
    limit,
    order,
    startAfter
  }: {
    limit?: number;
    order?: Ordering;
    startAfter?: number;
  }) => Promise<DaosResponse>;
}
export class IdentityserviceQueryClient implements IdentityserviceReadOnlyInterface {
  client: LCDClient;
  contractAddress: string;

  constructor(client: LCDClient, contractAddress: string) {
    this.client = client;
    this.contractAddress = contractAddress;
    this.getIdentityByOwner = this.getIdentityByOwner.bind(this);
    this.getIdentityByName = this.getIdentityByName.bind(this);
    this.daos = this.daos.bind(this);
  }

  getIdentityByOwner = async ({
    owner
  }: {
    owner: string;
  }): Promise<GetIdentityByOwnerResponse> => {
    return this.client.wasm.contractQuery(this.contractAddress, {
      get_identity_by_owner: {
        owner
      }
    });
  };
  getIdentityByName = async ({
    name
  }: {
    name: string;
  }): Promise<GetIdentityByNameResponse> => {
    return this.client.wasm.contractQuery(this.contractAddress, {
      get_identity_by_name: {
        name
      }
    });
  };
  daos = async ({
    limit,
    order,
    startAfter
  }: {
    limit?: number;
    order?: Ordering;
    startAfter?: number;
  }): Promise<DaosResponse> => {
    return this.client.wasm.contractQuery(this.contractAddress, {
      daos: {
        limit,
        order,
        start_after: startAfter
      }
    });
  };
}
export interface IdentityserviceInterface extends IdentityserviceReadOnlyInterface {
  contractAddress: string;
  registerUser: ({
    name
  }: {
    name: string;
  }, coins?: Coins) => Promise<WaitTxBroadcastResult>;
  registerDao: ({
    daoName,
    maxVotingPeriod,
    threshold,
    voters
  }: {
    daoName: string;
    maxVotingPeriod: Duration;
    threshold: Threshold;
    voters: Voter[];
  }, coins?: Coins) => Promise<WaitTxBroadcastResult>;
}
export class IdentityserviceClient extends IdentityserviceQueryClient implements IdentityserviceInterface {
  client: LCDClient;
  user: any;
  contractAddress: string;

  constructor(client: LCDClient, user: any, contractAddress: string) {
    super(client, contractAddress);
    this.client = client;
    this.user = user;
    this.contractAddress = contractAddress;
    this.registerUser = this.registerUser.bind(this);
    this.registerDao = this.registerDao.bind(this);
  }

  registerUser = async ({
    name
  }: {
    name: string;
  }, coins?: Coins): Promise<WaitTxBroadcastResult> => {
    const key = new MnemonicKey(this.user.mnemonicKeyOptions);
    const wallet = this.client.wallet(key);
    const execMsg = new MsgExecuteContract(this.user.address, this.contractAddress, {
      register_user: {
        name
      }
    }, coins);
    const txOptions = { msgs: [execMsg] };
    const tx = await wallet.createAndSignTx(txOptions);
    return await this.client.tx.broadcast(tx);
  };
  registerDao = async ({
    daoName,
    maxVotingPeriod,
    threshold,
    voters
  }: {
    daoName: string;
    maxVotingPeriod: Duration;
    threshold: Threshold;
    voters: Voter[];
  }, coins?: Coins): Promise<WaitTxBroadcastResult> => {
    const key = new MnemonicKey(this.user.mnemonicKeyOptions);
    const wallet = this.client.wallet(key);
    const execMsg = new MsgExecuteContract(this.user.address, this.contractAddress, {
      register_dao: {
        dao_name: daoName,
        max_voting_period: maxVotingPeriod,
        threshold,
        voters
      }
    }, coins);
    const txOptions = { msgs: [execMsg] };
    const tx = await wallet.createAndSignTx(txOptions);
    return await this.client.tx.broadcast(tx);
  };
}