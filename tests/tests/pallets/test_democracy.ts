import BigNumber from 'bignumber.js';
import { expect } from 'chai';

import { Keyring } from '@polkadot/api';
import { blake2AsHex } from '@polkadot/util-crypto';

import {
  AMOUNT_FACTOR, MIN_PROPOSE_AMOUNT, PREIMAGE_BASE_DEPOSIT,
  PREIMAGE_BYTE_DEPOSIT
} from '../../constants/currency';
import { TEST_CONTROLLERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';
import { endVote, jumpToLaunch } from '../utils';

import type { SubmittableExtrinsic } from '@polkadot/api/promise/types';

describeDevNode('pallet_democracy - note preimage', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  let encodedHash: string = '';

  it('should successfully register a preimage', async function () {
    const xt = context.polkadotApi.tx.bfcStaking.setMaxFullSelected(20);
    const encodedProposal = (xt as SubmittableExtrinsic)?.method.toHex() || '';
    const storageFee = new BigNumber(PREIMAGE_BASE_DEPOSIT).plus(new BigNumber(PREIMAGE_BYTE_DEPOSIT).multipliedBy(xt.length - 1));
    encodedHash = blake2AsHex(encodedProposal);

    await context.polkadotApi.tx.preimage
      .notePreimage(encodedProposal)
      .signAndSend(alith);

    await context.createBlock();

    const rawPreimage: any = await context.polkadotApi.query.preimage.preimageFor([encodedHash, xt.length - 1]);
    const preimage = rawPreimage.unwrap().toJSON();
    expect(preimage).to.not.be.null;

    const rawStatusFor: any = await context.polkadotApi.query.preimage.statusFor(encodedHash);
    const statusFor = rawStatusFor.unwrap().toJSON();

    expect(statusFor).has.key('unrequested');
    expect(statusFor.unrequested.deposit[0]).equal(alith.address);
    expect(new BigNumber(statusFor.unrequested.deposit[1]).toFixed()).equal(storageFee.toFixed());
    expect(statusFor.unrequested.len).equal(xt.length - 1);
  });

  it('should fail due to duplicate preimage', async function () {
    const xt = context.polkadotApi.tx.bfcStaking.setMaxFullSelected(20);
    const encodedProposal = (xt as SubmittableExtrinsic)?.method.toHex() || '';

    await context.polkadotApi.tx.preimage
      .notePreimage(encodedProposal)
      .signAndSend(alith);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'preimage', 'notePreimage');
    expect(extrinsicResult).equal('AlreadyNoted');
  });
});

describeDevNode('pallet_democracy - register public proposal', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  let encodedHash: string = '';
  let proposalLength: number = 0;

  before('generate preimage hash', async function () {
    const xt = context.polkadotApi.tx.bfcStaking.setMaxFullSelected(20);
    const encodedProposal = (xt as SubmittableExtrinsic)?.method.toHex() || '';
    encodedHash = blake2AsHex(encodedProposal);
    proposalLength = xt.length - 1;
  });

  it('should fail due to minimum deposit constraint', async function () {
    const value = new BigNumber(MIN_PROPOSE_AMOUNT).minus(10 ** 18);
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };

    await context.polkadotApi.tx.democracy
      .propose(request, value.toFixed())
      .signAndSend(alith);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'democracy', 'propose');
    expect(extrinsicResult).equal('ValueLow');
  });

  it('should successfully register a public proposal', async function () {
    const value = new BigNumber(MIN_PROPOSE_AMOUNT);
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };

    await context.polkadotApi.tx.democracy
      .propose(request, value.toFixed())
      .signAndSend(alith);

    await context.createBlock();

    const rawPublicPropCount: any = await context.polkadotApi.query.democracy.publicPropCount();
    const publicPropCount = rawPublicPropCount.toNumber();

    const rawDepositOf: any = await context.polkadotApi.query.democracy.depositOf(0);
    const depositOf = rawDepositOf.unwrap().toJSON();

    const rawPublicProps: any = await context.polkadotApi.query.democracy.publicProps();
    const publicProps = rawPublicProps.toJSON();

    expect(publicPropCount).equal(1);
    expect(depositOf[0][0]).equal(alith.address);
    expect(publicProps[0][0]).equal(0);
    expect(publicProps[0][1]).has.key('lookup');
    expect(publicProps[0][1].lookup.hash).equal(encodedHash);
    expect(publicProps[0][1].lookup.len).equal(proposalLength);
    expect(publicProps[0][2]).equal(alith.address);
  });
});

describeDevNode('pallet_democracy - endorse public proposal', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);

  before('should successfully register a public proposal', async function () {
    const xt = context.polkadotApi.tx.bfcStaking.setMaxFullSelected(20);
    const encodedProposal = (xt as SubmittableExtrinsic)?.method.toHex() || '';
    const encodedHash = blake2AsHex(encodedProposal);
    const proposalLength = xt.length - 1;

    await context.polkadotApi.tx.preimage
      .notePreimage(encodedProposal)
      .signAndSend(alith);

    await context.createBlock();

    const value = new BigNumber(MIN_PROPOSE_AMOUNT);
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };

    await context.polkadotApi.tx.democracy
      .propose(request, value.toFixed())
      .signAndSend(alith);

    await context.createBlock();
  });

  it('should fail due to wrong proposal index', async function () {
    const proposalIndex = 100;

    await context.polkadotApi.tx.democracy
      .second(proposalIndex)
      .signAndSend(baltathar);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'democracy', 'second');
    expect(extrinsicResult).equal('ProposalMissing');
  });

  it('should successfully endorse a public proposal', async function () {
    const proposalIndex = 0;

    await context.polkadotApi.tx.democracy
      .second(proposalIndex)
      .signAndSend(baltathar);

    await context.createBlock();

    const rawDepositOf: any = await context.polkadotApi.query.democracy.depositOf(0);
    const depositOf = rawDepositOf.unwrap().toJSON();

    expect(depositOf[0]).includes(baltathar.address);
  });

  it('should allow multiple endorsements for a single account', async function () {
    const proposalIndex = 0;

    await context.polkadotApi.tx.democracy
      .second(proposalIndex)
      .signAndSend(alith);

    await context.createBlock();

    const rawDepositOf: any = await context.polkadotApi.query.democracy.depositOf(0);
    const depositOf = rawDepositOf.unwrap().toJSON();
    const depositOfLength = depositOf[0].length;

    expect(depositOfLength).equal(3);
    expect(depositOf[0][0]).equal(alith.address);
    expect(depositOf[0][depositOfLength - 1]).equal(alith.address);
  });
});

describeDevNode('pallet_democracy - referendum interactions', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  let encodedHash: string = '';

  before('should successfully register a public proposal', async function () {
    const xt = context.polkadotApi.tx.bfcStaking.setMaxFullSelected(20);
    const encodedProposal = (xt as SubmittableExtrinsic)?.method.toHex() || '';
    encodedHash = blake2AsHex(encodedProposal);
    const proposalLength = xt.length - 1;

    await context.polkadotApi.tx.preimage
      .notePreimage(encodedProposal)
      .signAndSend(alith);

    await context.createBlock();

    const value = new BigNumber(MIN_PROPOSE_AMOUNT);
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };

    await context.polkadotApi.tx.democracy
      .propose(request, value.toFixed())
      .signAndSend(alith);

    await context.createBlock();
  });

  it('should successfully launch top endorsed public proposal', async function () {
    this.timeout(20000);

    await jumpToLaunch(context);

    const rawPublicProps: any = await context.polkadotApi.query.democracy.publicProps();
    const publicProps = rawPublicProps.toHuman();
    expect(publicProps.length).equal(0);

    const rawReferendumCount: any = await context.polkadotApi.query.democracy.referendumCount();
    const referendumCount = rawReferendumCount.toNumber();
    expect(referendumCount).equal(1);

    const rawReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const referendumInfo = rawReferendumInfo.unwrap().toJSON();
    expect(referendumInfo.ongoing.proposal.lookup.hash).equal(encodedHash);
  });

  it('should fail due to wrong referendum index', async function () {
    const referendumIndex = 100;

    const request = {
      vote: {
        aye: true,
        conviction: 2,
      },
      balance: AMOUNT_FACTOR,
    };

    await context.polkadotApi.tx.democracy
      .vote(referendumIndex, { Standard: request })
      .signAndSend(baltathar);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'democracy', 'vote');
    expect(extrinsicResult).equal('ReferendumInvalid');
  });

  it('should successfully vote for an aye', async function () {
    const referendumIndex = 0;

    const request = {
      vote: {
        aye: true,
        conviction: 2,
      },
      balance: AMOUNT_FACTOR,
    };

    await context.polkadotApi.tx.democracy
      .vote(referendumIndex, { Standard: request })
      .signAndSend(baltathar);

    await context.createBlock();

    const rawReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const referendumInfo = rawReferendumInfo.unwrap().toJSON();

    expect(referendumInfo.ongoing.tally.ayes).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(new BigNumber(AMOUNT_FACTOR).multipliedBy(2).toFixed()),
        32,
      ),
    );
    expect(referendumInfo.ongoing.tally.turnout).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );

    const rawLocks: any = await context.polkadotApi.query.balances.locks(baltathar.address);
    const locks = rawLocks.toHuman();
    expect(locks[0].id).equal('democrac');
    expect(locks[0].amount.replace(/,/g, '')).equal(AMOUNT_FACTOR);

    const rawVotingOf: any = await context.polkadotApi.query.democracy.votingOf(baltathar.address);
    const votingOf = rawVotingOf.toJSON();
    expect(votingOf.direct.votes[0][1].standard.balance).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
  });

  it('should successfully vote for a nay', async function () {
    const referendumIndex = 0;

    const request = {
      vote: {
        aye: false,
        conviction: 2,
      },
      balance: AMOUNT_FACTOR,
    };

    await context.polkadotApi.tx.democracy
      .vote(referendumIndex, { Standard: request })
      .signAndSend(charleth);

    await context.createBlock();

    const rawReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const referendumInfo = rawReferendumInfo.unwrap().toJSON();

    expect(referendumInfo.ongoing.tally.ayes).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(new BigNumber(AMOUNT_FACTOR).multipliedBy(2).toFixed()),
        32,
      ),
    );
    expect(referendumInfo.ongoing.tally.nays).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(new BigNumber(AMOUNT_FACTOR).multipliedBy(2).toFixed()),
        32,
      ),
    );
    expect(referendumInfo.ongoing.tally.turnout).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(new BigNumber(AMOUNT_FACTOR).multipliedBy(2).toFixed()),
        32,
      ),
    );

    const rawLocks: any = await context.polkadotApi.query.balances.locks(charleth.address);
    const locks = rawLocks.toHuman();
    expect(locks[0].id).equal('democrac');
    expect(locks[0].amount.replace(/,/g, '')).equal(AMOUNT_FACTOR);
  });

  it('should successfully vote for an aye with a Locked6x conviction', async function () {
    const referendumIndex = 0;

    const request = {
      vote: {
        aye: true,
        conviction: 6,
      },
      balance: AMOUNT_FACTOR,
    };

    await context.polkadotApi.tx.democracy
      .vote(referendumIndex, { Standard: request })
      .signAndSend(baltathar);

    await context.createBlock();

    const rawReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const referendumInfo = rawReferendumInfo.unwrap().toJSON();

    // the previous vote is replaced
    expect(referendumInfo.ongoing.tally.ayes).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(new BigNumber(AMOUNT_FACTOR).multipliedBy(6).toFixed()),
        32,
      ),
    );
    expect(referendumInfo.ongoing.tally.turnout).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(new BigNumber(AMOUNT_FACTOR).multipliedBy(2).toFixed()),
        32,
      ),
    );

    const rawVotingOf: any = await context.polkadotApi.query.democracy.votingOf(baltathar.address);
    const votingOf = rawVotingOf.toJSON();
    expect(votingOf.direct.votes[0][1].standard.balance).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
  });

  it('should successfully bake inapproved referendum', async function () {
    const rawReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const referendumInfo = rawReferendumInfo.unwrap().toJSON();

    const approves = new BigNumber(context.web3.utils.hexToNumberString(referendumInfo.ongoing.tally.ayes));
    const againsts = new BigNumber(context.web3.utils.hexToNumberString(referendumInfo.ongoing.tally.nays));
    const turnout = new BigNumber(context.web3.utils.hexToNumberString(referendumInfo.ongoing.tally.turnout));

    const rawElectorate: any = await context.polkadotApi.query.balances.totalIssuance();
    const electorate = new BigNumber(rawElectorate.toString());

    const isApproved = approves.dividedBy(electorate.sqrt()).isGreaterThan(againsts.dividedBy(turnout.sqrt()));

    await endVote(context, 0);
    await context.createBlock();

    const rawBakedReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const bakedReferendumInfo = rawBakedReferendumInfo.unwrap().toJSON();
    expect(bakedReferendumInfo.finished.approved).equal(isApproved);
  });

  it('should fail due to unlock request before vote removing', async function () {
    await context.polkadotApi.tx.democracy
      .unlock(baltathar.address)
      .signAndSend(baltathar);

    await context.createBlock();

    // remove_vote() must be requested priorly
    const rawLocks: any = await context.polkadotApi.query.balances.locks(baltathar.address);
    const locks = rawLocks.toHuman();
    expect(locks[0].id).equal('democrac');
    expect(locks[0].amount.replace(/,/g, '')).equal(AMOUNT_FACTOR);
  });

  it('should successfully remove vote for loser', async function () {
    const referendumIndex = 0;

    await context.polkadotApi.tx.democracy
      .removeVote(referendumIndex)
      .signAndSend(baltathar);

    await context.createBlock();

    // clean removal for voter who lost
    const rawVotingOf: any = await context.polkadotApi.query.democracy.votingOf(baltathar.address);
    const votingOf = rawVotingOf.toJSON();
    expect(votingOf.direct.votes).to.be.empty;
  });

  it('should successfully remove vote for winner', async function () {
    const referendumIndex = 0;

    await context.polkadotApi.tx.democracy
      .removeVote(referendumIndex)
      .signAndSend(charleth);

    await context.createBlock();

    const rawVoteLockingPeriod: any = context.polkadotApi.consts.democracy.voteLockingPeriod;
    // locking period * conviction
    const voteLockingPeriod = rawVoteLockingPeriod.toNumber() * 2;

    const rawBakedReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const bakedReferendumInfo = rawBakedReferendumInfo.unwrap().toJSON();

    const voteLockEndsAt = bakedReferendumInfo.finished.end + voteLockingPeriod;

    // adds lock info to votingOf
    const rawVotingOf: any = await context.polkadotApi.query.democracy.votingOf(charleth.address);
    const votingOf = rawVotingOf.toJSON();
    expect(votingOf.direct.prior[0]).equal(voteLockEndsAt);
    expect(votingOf.direct.prior[1]).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
  });

  it('should successfully unlock locked balance for loser', async function () {
    await context.polkadotApi.tx.democracy
      .unlock(baltathar.address)
      .signAndSend(baltathar);

    await context.createBlock();

    const rawLocks: any = await context.polkadotApi.query.balances.locks(baltathar.address);
    const locks = rawLocks.toHuman();
    expect(locks).to.be.empty;
  });

  it('should fail to unlock locked balance before lock period ends', async function () {
    await context.polkadotApi.tx.democracy
      .unlock(charleth.address)
      .signAndSend(charleth);

    await context.createBlock();

    const rawLocks: any = await context.polkadotApi.query.balances.locks(charleth.address);
    const locks = rawLocks.toHuman();
    expect(locks).to.be.not.empty;

    // unlock available only when locking period ends
    const rawVotingOf: any = await context.polkadotApi.query.democracy.votingOf(charleth.address);
    const votingOf = rawVotingOf.toJSON();
    expect(votingOf.direct.prior).to.be.not.empty;
  });
});

describeDevNode('pallet_democracy - delegation', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  before('should successfully register a public proposal', async function () {
    this.timeout(20000);

    const xt = context.polkadotApi.tx.bfcStaking.setMaxFullSelected(20);
    const encodedProposal = (xt as SubmittableExtrinsic)?.method.toHex() || '';
    const encodedHash = blake2AsHex(encodedProposal);
    const proposalLength = xt.length - 1;

    await context.polkadotApi.tx.preimage
      .notePreimage(encodedProposal)
      .signAndSend(alith);

    await context.createBlock();

    const value = new BigNumber(MIN_PROPOSE_AMOUNT);
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };

    await context.polkadotApi.tx.democracy
      .propose(request, value.toFixed())
      .signAndSend(alith);

    await context.createBlock();

    await jumpToLaunch(context);
  });

  it('should fail due to self-delegation', async function () {
    const conviction = 6;

    await context.polkadotApi.tx.democracy
      .delegate(charleth.address, conviction, AMOUNT_FACTOR)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'democracy', 'delegate');
    expect(extrinsicResult).equal('Nonsense');
  });

  it('should successfully delegate baltathar', async function () {
    const conviction = 6;

    await context.polkadotApi.tx.democracy
      .delegate(baltathar.address, conviction, AMOUNT_FACTOR)
      .signAndSend(charleth);

    await context.createBlock();

    const rawFrom: any = await context.polkadotApi.query.democracy.votingOf(charleth.address);
    const from = rawFrom.toJSON();
    expect(from.delegating.balance).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
    expect(from.delegating.target).equal(baltathar.address);
    expect(from.delegating.conviction).equal('Locked6x');

    const rawTo: any = await context.polkadotApi.query.democracy.votingOf(baltathar.address);
    const to = rawTo.toJSON();
    expect(to.direct.delegations.votes).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(new BigNumber(AMOUNT_FACTOR).multipliedBy(conviction).toFixed()),
        32,
      ),
    );
    expect(to.direct.delegations.capital).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
  });

  it('should successfully vote with delegation', async function () {
    const referendumIndex = 0;

    const request = {
      vote: {
        aye: true,
        conviction: 1,
      },
      balance: AMOUNT_FACTOR,
    };

    await context.polkadotApi.tx.democracy
      .vote(referendumIndex, { Standard: request })
      .signAndSend(baltathar);

    await context.createBlock();

    const conviction = 6;
    const delegation = new BigNumber(AMOUNT_FACTOR).multipliedBy(conviction);
    const ayes = new BigNumber(AMOUNT_FACTOR).plus(delegation);
    const turnout = new BigNumber(AMOUNT_FACTOR).plus(AMOUNT_FACTOR);

    const rawReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const referendumInfo = rawReferendumInfo.unwrap().toJSON();

    expect(referendumInfo.ongoing.tally.ayes).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(ayes.toFixed()),
        32,
      ),
    );
    expect(referendumInfo.ongoing.tally.turnout).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(turnout.toFixed()),
        32,
      ),
    );

    const rawLocksForVoter: any = await context.polkadotApi.query.balances.locks(baltathar.address);
    const locksForVoter = rawLocksForVoter.toHuman();
    expect(locksForVoter[0].id).equal('democrac');
    expect(locksForVoter[0].amount.replace(/,/g, '')).equal(AMOUNT_FACTOR);

    const rawLocksForDelegator: any = await context.polkadotApi.query.balances.locks(charleth.address);
    const locksForDelegator = rawLocksForDelegator.toHuman();
    expect(locksForDelegator[0].id).equal('democrac');
    expect(locksForDelegator[0].amount.replace(/,/g, '')).equal(AMOUNT_FACTOR);
  });

  it('should successfully undelegate', async function () {
    // balance lock remains - unlock() required
    await context.polkadotApi.tx.democracy
      .undelegate()
      .signAndSend(charleth);

    await context.createBlock();

    const rawReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const referendumInfo = rawReferendumInfo.unwrap().toJSON();
    expect(referendumInfo.ongoing.tally.ayes).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
    expect(referendumInfo.ongoing.tally.turnout).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
  });

  it('should successfully change delegation target', async function () {
    const conviction = 6;

    await context.polkadotApi.tx.democracy
      .delegate(alith.address, conviction, AMOUNT_FACTOR)
      .signAndSend(charleth);

    await context.createBlock();

    const rawFrom: any = await context.polkadotApi.query.democracy.votingOf(charleth.address);
    const from = rawFrom.toJSON();
    expect(from.delegating.balance).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
    expect(from.delegating.target).equal(alith.address);
    expect(from.delegating.conviction).equal('Locked6x');

    const rawToBefore: any = await context.polkadotApi.query.democracy.votingOf(baltathar.address);
    const toBefore = rawToBefore.toJSON();
    expect(toBefore.direct.delegations.votes).equal(0);
    expect(toBefore.direct.delegations.capital).equal(0);

    const rawToAfter: any = await context.polkadotApi.query.democracy.votingOf(alith.address);
    const toAfter = rawToAfter.toJSON();
    expect(toAfter.direct.delegations.votes).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(new BigNumber(AMOUNT_FACTOR).multipliedBy(conviction).toFixed()),
        32,
      ),
    );
    expect(toAfter.direct.delegations.capital).equal(
      context.web3.utils.padLeft(
        context.web3.utils.toHex(AMOUNT_FACTOR),
        32,
      ),
    );
  });

  it('should fail due to delegation from an account who already voted', async function () {
    const conviction = 6;

    await context.polkadotApi.tx.democracy
      .delegate(alith.address, conviction, AMOUNT_FACTOR)
      .signAndSend(baltathar);

    await context.createBlock();

    // cannot delegate with an account who already voted
    const extrinsicResult = await getExtrinsicResult(context, 'democracy', 'delegate');
    expect(extrinsicResult).equal('VotesExist');
  });

  it('should fail to vote due to delegation', async function () {
    const referendumIndex = 0;

    const request = {
      vote: {
        aye: true,
        conviction: 1,
      },
      balance: AMOUNT_FACTOR,
    };

    await context.polkadotApi.tx.democracy
      .vote(referendumIndex, { Standard: request })
      .signAndSend(charleth);

    await context.createBlock();

    // cannot vote with an account who already delegated - undelegate() required
    const extrinsicResult = await getExtrinsicResult(context, 'democracy', 'vote');
    expect(extrinsicResult).equal('AlreadyDelegating');
  });
});
