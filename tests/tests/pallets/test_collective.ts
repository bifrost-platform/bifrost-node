import BigNumber from 'bignumber.js';
import { expect } from 'chai';

import { Keyring } from '@polkadot/api';
import { blake2AsHex } from '@polkadot/util-crypto';

import { MIN_PROPOSE_AMOUNT } from '../../constants/currency';
import { TEST_CONTROLLERS } from '../../constants/keys';
import { getExtrinsicResult, isEventTriggered } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';
import { jumpToLaunch } from '../utils';

import type { SubmittableExtrinsic } from '@polkadot/api/promise/types';

describeDevNode('pallet_collective - prime member', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);

  it('should successfully set prime tc member', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.technicalMembership.setPrime(alith.address)
    ).signAndSend(alith);

    await context.createBlock();

    const rawPrimeM: any = await context.polkadotApi.query.technicalMembership.prime();
    const primeM = rawPrimeM.unwrap().toJSON();
    expect(primeM).equal(alith.address);

    const rawPrimeC: any = await context.polkadotApi.query.technicalCommittee.prime();
    const primeC = rawPrimeC.unwrap().toJSON();
    expect(primeC).equal(alith.address);
  });
});

// council proposal == motion
describeDevNode('pallet_collective - council proposal interaction', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);

  let encodedHash: string = '';
  let proposalHash: string = '';
  let proposalLength: number = 0;

  before('should successfully register a preimage', async function () {
    const xt = context.polkadotApi.tx.bfcStaking.setMaxFullSelected(20);
    const encodedProposal = (xt as SubmittableExtrinsic)?.method.toHex() || '';
    encodedHash = blake2AsHex(encodedProposal);
    proposalLength = xt.length;

    await context.polkadotApi.tx.preimage
      .notePreimage(encodedProposal)
      .signAndSend(alith);
    await context.createBlock();
  });

  it('should fail due to invalid length bound', async function () {
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };
    const proposalXt = context.polkadotApi.tx.democracy.externalProposeMajority(request);
    await context.polkadotApi.tx.council
      .propose(3, proposalXt, 0)
      .signAndSend(alith);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'council', 'propose');
    expect(extrinsicResult).equal('WrongProposalLength');
  });

  it('should fail due to non-council member', async function () {
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };
    const proposalXt = context.polkadotApi.tx.democracy.externalProposeMajority(request);
    await context.polkadotApi.tx.council
      .propose(3, proposalXt, 1000)
      .signAndSend(dorothy);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'council', 'propose');
    expect(extrinsicResult).equal('NotMember');
  });

  it('should successfully register an external proposal', async function () {
    // if threshold < 2, it will execute proposal with normal origin
    // else, it will start a council proposal for council members to vote
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };
    const proposalXt = context.polkadotApi.tx.democracy.externalProposeMajority(request);
    proposalHash = ((proposalXt as SubmittableExtrinsic)?.method.hash || '').toHex();

    await context.polkadotApi.tx.council
      .propose(3, proposalXt, 1000)
      .signAndSend(alith);

    await context.createBlock();

    const rawProposals: any = await context.polkadotApi.query.council.proposals();
    const proposals = rawProposals.toJSON();
    expect(proposals).is.not.empty;
    expect(proposals[0]).equal(proposalHash);

    const rawProposalOf: any = await context.polkadotApi.query.council.proposalOf(proposals[0]);
    const proposalOf = rawProposalOf.toJSON();
    expect(proposalOf.args.proposal.lookup.hash).equal(encodedHash);
  });

  it('should fail due to duplicate proposal', async function () {
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };
    const proposalXt = context.polkadotApi.tx.democracy.externalProposeMajority(request);
    await context.polkadotApi.tx.council
      .propose(3, proposalXt, 1000)
      .signAndSend(alith);

    await context.createBlock();
    const extrinsicResult = await getExtrinsicResult(context, 'council', 'propose');
    expect(extrinsicResult).equal('DuplicateProposal');
  });

  it('should successfully vote an aye', async function () {
    const proposalIndex = 0;
    const approve = true;
    await context.polkadotApi.tx.council
      .vote(proposalHash, proposalIndex, approve)
      .signAndSend(alith);

    await context.createBlock();

    const rawVotingOf: any = await context.polkadotApi.query.council.voting(proposalHash);
    const votingOf = rawVotingOf.toJSON();
    expect(votingOf.ayes[0]).equal(alith.address);
  });

  it('should successfully be closed - disapproved', async function () {
    const proposalIndex = 0;
    const approve = false;

    await context.polkadotApi.tx.council
      .vote(proposalHash, proposalIndex, approve)
      .signAndSend(baltathar);
    await context.createBlock();

    await context.polkadotApi.tx.council
      .vote(proposalHash, proposalIndex, approve)
      .signAndSend(charleth);
    await context.createBlock();

    await context.polkadotApi.tx.council
      .close(proposalHash, proposalIndex, { ref_time: 1000, proof_size: 1000 }, 1000)
      .signAndSend(alith);
    const block = await context.createBlock();

    // it should receive more than the threshold of aye votes
    const success = await isEventTriggered(
      context,
      block.block.hash,
      [
        { method: 'Closed', section: 'council' },
        { method: 'Disapproved', section: 'council' },
      ],
    );
    expect(success).equal(true);
  });

  it('should successfully be closed - approved', async function () {
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };
    const proposalXt = context.polkadotApi.tx.democracy.externalProposeMajority(request);
    proposalHash = ((proposalXt as SubmittableExtrinsic)?.method.hash || '').toHex();
    const proposalLengthV2 = proposalXt.length;

    await context.polkadotApi.tx.council
      .propose(3, proposalXt, proposalLengthV2)
      .signAndSend(alith);
    await context.createBlock();

    const proposalIndex = 1;
    const approve = true;

    await context.polkadotApi.tx.council
      .vote(proposalHash, proposalIndex, approve)
      .signAndSend(alith);
    await context.createBlock();

    await context.polkadotApi.tx.council
      .vote(proposalHash, proposalIndex, approve)
      .signAndSend(baltathar);
    await context.createBlock();

    await context.polkadotApi.tx.council
      .vote(proposalHash, proposalIndex, approve)
      .signAndSend(charleth);
    await context.createBlock();

    const proposalWeight = (await proposalXt.paymentInfo(alith)).weight;

    await context.polkadotApi.tx.council
      .close(proposalHash, proposalIndex, proposalWeight, proposalLengthV2)
      .signAndSend(alith);
    const block = await context.createBlock();

    // it should receive more than the threshold of aye votes
    const success = await isEventTriggered(
      context,
      block.block.hash,
      [
        { method: 'Closed', section: 'council' },
        { method: 'Approved', section: 'council' },
      ],
    );
    expect(success).equal(true);

    const rawNextExternal: any = await context.polkadotApi.query.democracy.nextExternal();
    const nextExternal = rawNextExternal.toJSON();
    expect(nextExternal[0].lookup.hash).equal(encodedHash);
  });

  it('should successfully launch an external proposal', async function () {
    this.timeout(20000);

    await jumpToLaunch(context);

    const rawReferendumInfoOf: any = await context.polkadotApi.query.democracy.referendumInfoOf(0);
    const referendumInfo = rawReferendumInfoOf.toJSON();
    expect(referendumInfo.ongoing.proposal.lookup.hash).equal(encodedHash);
  });
});

describeDevNode('pallet_collective - proposal cancellation', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  let encodedHash: string = '';
  let proposalLength: number = 0;

  // 1. cancel_proposal - not passed - CancelProposalOrigin - TC
  // 2. cancel_queued - cancel enactment - Root (council proposal required)
  // 3. cancel_referendum - passed - Root (council proposal required)
  // 4. clear_public_proposals - not passed - Root (council proposal required)
  // 5. emergency_cancel - passed - CancellationOrigin - Council

  before('should successfully register a preimage', async function () {
    const xt = context.polkadotApi.tx.bfcStaking.setMaxFullSelected(20);
    const encodedProposal = (xt as SubmittableExtrinsic)?.method.toHex() || '';
    encodedHash = blake2AsHex(encodedProposal);
    proposalLength = xt.length - 1;

    await context.polkadotApi.tx.preimage
      .notePreimage(encodedProposal)
      .signAndSend(alith);

    await context.createBlock();
  });

  beforeEach('should successfully register a public proposal', async function () {
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

  it('should successfully cancel public proposal', async function () {
    // propose cancellation of a public proposal - democracy.externalProposeMajority
    const cancelProposal = context.polkadotApi.tx.democracy.cancelProposal(0);
    const proposalWeight = (await cancelProposal.paymentInfo(alith)).weight;
    const proposalLength = cancelProposal.length;

    await context.polkadotApi.tx.technicalCommittee
      .propose(2, cancelProposal, proposalLength)
      .signAndSend(alith);
    await context.createBlock();

    const cancelProposalHash = ((cancelProposal as SubmittableExtrinsic)?.method.hash || '').toHex();
    const cancelProposalIndex = 0;
    const approve = true;

    // vote
    await context.polkadotApi.tx.technicalCommittee
      .vote(cancelProposalHash, cancelProposalIndex, approve)
      .signAndSend(alith);
    await context.createBlock();

    await context.polkadotApi.tx.technicalCommittee
      .vote(cancelProposalHash, cancelProposalIndex, approve)
      .signAndSend(baltathar);
    await context.createBlock();

    // close when threshold reached
    await context.polkadotApi.tx.technicalCommittee
      .close(cancelProposalHash, cancelProposalIndex, proposalWeight, proposalLength)
      .signAndSend(alith);
    const block = await context.createBlock();

    // check result
    // 1. balance slashed
    // 2. proposal cancelled
    const success = await isEventTriggered(
      context,
      block.block.hash,
      [
        { method: 'Closed', section: 'technicalCommittee' },
        { method: 'Approved', section: 'technicalCommittee' },
        { method: 'Slashed', section: 'balances' },
        { method: 'Executed', section: 'technicalCommittee' },
      ],
    );
    expect(success).equal(true);

    const rawPublicProps: any = await context.polkadotApi.query.democracy.publicProps();
    const publicProps = rawPublicProps.toHuman();
    expect(publicProps).to.be.empty;
  });

  it('should successfully clear public proposals', async function () {
    // create preimage for clear_public_proposals()
    const clearProposals = context.polkadotApi.tx.democracy.clearPublicProposals();
    const encodedProposal = (clearProposals as SubmittableExtrinsic)?.method.toHex() || '';
    const encodedHash = blake2AsHex(encodedProposal);

    await context.polkadotApi.tx.preimage
      .notePreimage(encodedProposal)
      .signAndSend(alith);
    await context.createBlock();

    // create external proposal for clear_public_proposals()
    const request = {
      'Lookup': {
        hash: encodedHash,
        len: proposalLength,
      }
    };
    const externalProposal = context.polkadotApi.tx.democracy.externalProposeMajority(request);
    const proposalWeight = (await externalProposal.paymentInfo(alith)).weight;
    const proposalLengthV2 = externalProposal.length;

    await context.polkadotApi.tx.council
      .propose(2, externalProposal, proposalLengthV2)
      .signAndSend(alith);
    await context.createBlock();

    const externalProposalHash = ((externalProposal as SubmittableExtrinsic)?.method.hash || '').toHex();
    const externalProposalIndex = 0;
    const approve = true;

    // vote
    await context.polkadotApi.tx.council
      .vote(externalProposalHash, externalProposalIndex, approve)
      .signAndSend(baltathar);
    await context.createBlock();

    await context.polkadotApi.tx.council
      .vote(externalProposalHash, externalProposalIndex, approve)
      .signAndSend(charleth);
    await context.createBlock();

    // close when threshold reached
    await context.polkadotApi.tx.council
      .close(externalProposalHash, externalProposalIndex, proposalWeight, proposalLengthV2)
      .signAndSend(alith);
    const block = await context.createBlock();

    // check if external proposal is approved
    const success = await isEventTriggered(
      context,
      block.block.hash,
      [
        { method: 'Closed', section: 'council' },
        { method: 'Approved', section: 'council' },
        { method: 'Executed', section: 'council' },
      ],
    );
    expect(success).equal(true);

    // fast-track external proposal
    // vote on tc proposal for fast-track approvement
    // if passed, vote on referendum
    // if passed, wait for proposal enactment
  });
});
