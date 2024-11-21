import BigNumber from 'bignumber.js';
import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import {
  DEFAULT_STAKING_AMOUNT, MIN_NOMINATOR_STAKING_AMOUNT
} from '../../constants/currency';
import { TEST_CONTROLLERS, TEST_STASHES } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';
import { jumpToRound } from '../utils';

describeDevNode('pallet_bfc_staking - nominations', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  it('should fail due to minimum amount constraint', async function () {
    const stakeBelowMin = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT).minus(10 ** 15);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stakeBelowMin.toFixed(), 0, 0)
      .signAndSend(baltathar);

    await context.createBlock();
  });

  it('should fail due to unknown candidate', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(charleth.address, stake.toFixed(), 0, 0)
      .signAndSend(baltathar);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
    expect(extrinsicResult).equal('CandidateDNE');
  });

  it('should successfully nominate to alith - baltathar', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 0, 0)
      .signAndSend(baltathar);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();

    expect(nominatorState.nominations).has.key(alith.address);
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorState.initialNominations[alith.address].toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(stake.toFixed());

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();
    expect(candidateState.nominationCount.toString()).equal('1');
    expect(candidateState.lowestTopNominationAmount.toString()).equal(stake.toFixed());

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(stake);
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());

    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap();
    expect(topNominations.nominations.length).equal(1);
    expect(topNominations.nominations[0].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
    expect(topNominations.nominations[0].amount.toString()).equal(stake.toFixed());
  });

  it('should successfully nominate to alith - charleth', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(charleth);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();

    expect(nominatorState.nominations).has.key(alith.address);
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorState.initialNominations[alith.address].toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(stake.toFixed());

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();
    expect(candidateState.nominationCount.toString()).equal('2');
    expect(candidateState.lowestTopNominationAmount.toString()).equal(stake.toFixed());

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(stake.multipliedBy(2)); // for baltathar and charleth
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());

    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap();
    expect(topNominations.nominations.length).equal(2);
    expect(topNominations.nominations[0].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
    expect(topNominations.nominations[0].amount.toString()).equal(stake.toFixed());
    expect(topNominations.nominations[1].owner.toString().toLowerCase()).equal(charleth.address.toLowerCase());
    expect(topNominations.nominations[1].amount.toString()).equal(stake.toFixed());
  });

  it('should fail due to calling nominate function twice', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 1, 1)
      .signAndSend(baltathar);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
    expect(extrinsicResult).equal('AlreadyNominatedCandidate');
  });

  it('should fail due to calling nominatorBondMore before nominate', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominatorBondMore(alith.address, stake.toFixed())
      .signAndSend(dorothy);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominatorBondMore');
    expect(extrinsicResult).equal('NominatorDNE');
  });

  it('should successfully bond more', async function () {
    const more = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const stakeBefore = more;

    await context.polkadotApi.tx.bfcStaking
      .nominatorBondMore(alith.address, more.toFixed())
      .signAndSend(baltathar);

    await context.createBlock();

    const stakeAfter = more.multipliedBy(2); // we nominated twice with the same amount

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();

    expect(nominatorState.nominations).has.key(alith.address);
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stakeAfter.toFixed());
    expect(new BigNumber(nominatorState.initialNominations[alith.address].toString()).toFixed()).equal(stakeBefore.toFixed());
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(stakeAfter.toFixed());

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();
    expect(candidateState.nominationCount.toString()).equal('2');
    expect(candidateState.lowestTopNominationAmount.toString()).equal(more.toFixed());

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(more.multipliedBy(3)); // for baltathar (2) and charleth (1)
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());

    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap();
    expect(topNominations.nominations.length).equal(2);
    expect(topNominations.nominations[0].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
    expect(topNominations.nominations[0].amount.toString()).equal(stakeAfter.toFixed());
  });

  it('should successfully join bottom nominations', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(dorothy);

    await context.createBlock();

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();
    expect(candidateState.nominationCount.toString()).equal('3');
    expect(candidateState.lowestTopNominationAmount.toString()).equal(stake.toFixed());
    expect(candidateState.highestBottomNominationAmount.toString()).equal(stake.toFixed());
    expect(candidateState.lowestBottomNominationAmount.toString()).equal(stake.toFixed());

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(stake.multipliedBy(3)); // for baltathar (2) and charleth (1)
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed()); // voting power includes top only

    const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatePool = rawCandidatePool.toJSON();
    expect(new BigNumber(candidatePool[alith.address].toString()).toFixed()).equal(expectedStake.toFixed()); // CandidatePool includes top only

    const rawTotal: any = await context.polkadotApi.query.bfcStaking.total();
    const total = rawTotal.toJSON();
    expect(new BigNumber(total.toString()).toFixed()).equal(expectedStake.plus(stake).toFixed()); // Total includes both top and bottom

    const rawBottomNominations: any = await context.polkadotApi.query.bfcStaking.bottomNominations(alith.address);
    const bottomNominations = rawBottomNominations.unwrap();
    expect(bottomNominations.nominations.length).equal(1);
    expect(bottomNominations.nominations[0].owner.toString().toLowerCase()).equal(dorothy.address.toLowerCase());
    expect(bottomNominations.nominations[0].amount.toString()).equal(stake.toFixed());
  });

  it('should successfully schedule nominator bond less', async function () {
    const less = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const stakeAfter = less;

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap();
    const votingPowerBefore = new BigNumber(candidateStateBefore.votingPower.toString());

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(alith.address, less.toFixed())
      .signAndSend(baltathar);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();

    const nominatorState = rawNominatorState.unwrap().toJSON();
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stakeAfter.toFixed());
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(stakeAfter.toFixed());

    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap();
    expect(topNominations.nominations[0].amount.toString()).equal(stakeAfter.toFixed());

    const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateStateAfter = rawCandidateStateAfter.unwrap();
    let votingPowerAfter = new BigNumber(candidateStateAfter.votingPower.toString());
    expect(votingPowerAfter.toFixed()).equal(votingPowerBefore.minus(less).toFixed());

    const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatePool = rawCandidatePool.toJSON();
    expect(new BigNumber(candidatePool[alith.address].toString()).toFixed()).equal(votingPowerBefore.minus(less).toFixed());

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(less.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(less.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(less.toFixed());
    expect(nominatorRequests.requests[alith.address].action).equal('Decrease');
  });

  it('should successfully schedule nominator bond less multiple times - same round', async function () {
    const prevLess = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const less = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(2); // 500 BFC

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(alith.address, less.toFixed())
      .signAndSend(baltathar);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
    const nominatorState = rawNominatorState.unwrap();
    const nominatorRequests = nominatorState.requests.toJSON();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    // if requested multiple times in the same round, the amount should be cumulative
    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(less.plus(prevLess).toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(less.plus(prevLess).toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(less.plus(prevLess).toFixed());
    expect(nominatorRequests.requests[alith.address].action).equal('Decrease');
  });

  it('should successfully schedule nominator bond less multiple times - different rounds', async function () {
    const prevLess = new BigNumber(DEFAULT_STAKING_AMOUNT).plus(new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(2)); // 1500 BFC
    const less = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(alith.address, less.toFixed())
      .signAndSend(baltathar);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
    const nominatorState = rawNominatorState.unwrap();
    const nominatorRequests = nominatorState.requests.toJSON();

    // if requested multiple times in different rounds, it is individually stored
    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(less.plus(prevLess).toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(less.plus(prevLess).toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 2].toString()).toFixed()).equal(less.toFixed());
    expect(Object.keys(nominatorRequests.requests[alith.address].whenExecutable).length).equal(2);
    expect(nominatorRequests.requests[alith.address].action).equal('Decrease');

    // it should be moved to bottom nominations
    const rawBottomNominations: any = await context.polkadotApi.query.bfcStaking.bottomNominations(alith.address);
    const bottomNominations = rawBottomNominations.unwrap();
    expect(bottomNominations.nominations.length).equal(1);
    expect(bottomNominations.nominations[0].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());

    const rawUnstakingNominations: any = await context.polkadotApi.query.bfcStaking.unstakingNominations(alith.address);
    const unstakingNominations = rawUnstakingNominations.unwrap().toJSON();
    expect(unstakingNominations.nominations.length).equal(1);
    expect(unstakingNominations.nominations[0].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
    expect(new BigNumber(unstakingNominations.nominations[0].amount.toString()).toFixed()).equal(less.plus(prevLess).toFixed());
  });

  it('should fail to execute nominator bond less due to unknown when', async function () {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    const when = currentRound + 10;

    await context.polkadotApi.tx.bfcStaking
      .executeNominationRequest(alith.address, when)
      .signAndSend(baltathar);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeNominationRequest');
    expect(extrinsicResult).equal('PendingNominationRequestDNE');
  });

  it('should fail to execute nominator bond less due still pending', async function () {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    const when = currentRound + 1; // round 3

    await context.polkadotApi.tx.bfcStaking
      .executeNominationRequest(alith.address, when)
      .signAndSend(baltathar);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeNominationRequest');
    expect(extrinsicResult).equal('PendingNominationRequestNotDueYet');
  });

  it('should successfully execute nominator bond less', async function () {
    const reserved = new BigNumber(DEFAULT_STAKING_AMOUNT).multipliedBy(2);
    const orderAmount = new BigNumber(DEFAULT_STAKING_AMOUNT).plus(new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(2)); // 1500 BFC

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    const when = currentRound; // round 2

    const accountBefore = await context.polkadotApi.query.system.account(baltathar.address);
    expect(accountBefore['data'].reserved.toString()).equal(reserved.toFixed());

    await context.polkadotApi.tx.bfcStaking
      .executeNominationRequest(alith.address, when)
      .signAndSend(baltathar);

    await context.createBlock();

    const accountAfter = await context.polkadotApi.query.system.account(baltathar.address);
    expect(accountAfter['data'].reserved.toString()).equal(reserved.minus(orderAmount).toFixed());

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
    const nominatorState = rawNominatorState.unwrap();
    const nominatorRequests = nominatorState.requests.toJSON();

    const lessTotal = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC
    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(lessTotal.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(lessTotal.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(lessTotal.toFixed());
    expect(Object.keys(nominatorRequests.requests[alith.address].whenExecutable).length).equal(1);
  });

  it('should successfully kick out lowest bottom nomination - baltathar kicked out by ethan', async function () {
    const defaultStake = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const stake = defaultStake.multipliedBy(2); // make sure it goes to top

    const accountBefore = await context.polkadotApi.query.system.account(baltathar.address);
    expect(accountBefore['data'].reserved.toString()).equal(new BigNumber(500).multipliedBy(10 ** 18).toFixed());

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    // ethan joins top
    // dorothy moves to bottom
    // baltathar is kicked out

    // the reserved stake will be immediately returned - including any other pending requests
    const accountAfter = await context.polkadotApi.query.system.account(baltathar.address);
    expect(accountAfter['data'].reserved.toString()).equal(new BigNumber(0).toFixed());

    // nominator state should be deleted
    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
    const nominatorState = rawNominatorState.toJSON();
    expect(nominatorState).is.null;

    // top nominations should be updated - ethan joined, dorothy moved to bottom
    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap().toJSON();
    expect(topNominations.nominations[0].owner.toLowerCase()).equal(ethan.address.toLowerCase());
    expect(topNominations.nominations[1].owner.toLowerCase()).equal(charleth.address.toLowerCase());
    expect(topNominations.nominations.length).equal(2);
    expect(new BigNumber(topNominations.total.toString()).toFixed()).equal(stake.plus(defaultStake).toFixed());

    // bottom nominations should be updated - dorothy moved to bottom, baltathar kicked out
    const rawBottomNominations: any = await context.polkadotApi.query.bfcStaking.bottomNominations(alith.address);
    const bottomNominations = rawBottomNominations.unwrap().toJSON();
    expect(bottomNominations.nominations[0].owner.toLowerCase()).equal(dorothy.address.toLowerCase());
    expect(bottomNominations.nominations.length).equal(1);
    expect(new BigNumber(bottomNominations.total.toString()).toFixed()).equal(defaultStake.toFixed());
  });

  it('should successfully receive round rewards while decreasing stake', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT).multipliedBy(2); // 2000 BFC
    const less = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(alith.address, less.toFixed())
      .signAndSend(ethan);

    const accountBefore = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountBefore['data'].reserved.toString()).equal(stake.toFixed());

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    // we jump two rounds to ensure the nomination is included in the next payout
    await jumpToRound(context, currentRound + 2);

    const accountAfter = await context.polkadotApi.query.system.account(ethan.address);
    // the reserved stake should have increased due to the round rewards
    expect(new BigNumber(accountAfter['data'].reserved.toString()).gt(new BigNumber(accountBefore['data'].reserved.toString()))).is.true;
  });

  it('should successfully cancel nomination request - decrease (ethan)', async function () {
    const less = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC

    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap().toJSON();
    const nominatorRequestsBefore = rawNominatorStateBefore.unwrap().requests.toJSON();
    const stakeBefore = new BigNumber(nominatorStateBefore.nominations[alith.address].toString());

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = rawTotalBefore.toJSON();

    const rawTopNominationsBefore: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominationsBefore = rawTopNominationsBefore.unwrap().toJSON();

    const when = parseInt(Object.keys(nominatorRequestsBefore.requests[alith.address].whenExecutable)[0]);

    await context.polkadotApi.tx.bfcStaking
      .cancelNominationRequest(alith.address, when)
      .signAndSend(ethan);

    await context.createBlock();

    // nominator state should be rollbacked
    const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateAfter = rawNominatorStateAfter.unwrap().toJSON();
    const nominatorRequestsAfter = rawNominatorStateAfter.unwrap().requests.toJSON();
    expect(nominatorRequestsAfter.requests).is.empty;
    expect(new BigNumber(nominatorStateAfter.nominations[alith.address].toString()).toFixed()).equal(stakeBefore.plus(less).toFixed());
    expect(new BigNumber(nominatorStateAfter.total.toString()).toFixed()).equal(stakeBefore.plus(less).toFixed());

    // total should be rollbacked
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = rawTotalAfter.toJSON();
    expect(new BigNumber(totalAfter.toString()).toFixed()).equal(new BigNumber(totalBefore.toString()).plus(less).toFixed());

    // top nominations should be rollbacked
    const rawTopNominationsAfter: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominationsAfter = rawTopNominationsAfter.unwrap().toJSON();
    expect(new BigNumber(topNominationsAfter.total.toString()).toFixed()).equal(new BigNumber(topNominationsBefore.total.toString()).plus(less).toFixed());
    expect(new BigNumber(topNominationsAfter.nominations[0].amount.toString()).toFixed()).equal(new BigNumber(topNominationsBefore.nominations[0].amount.toString()).plus(less).toFixed());
  });

  it('should successfully schedule a revoke request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap();
    const revoke = new BigNumber(nominatorStateBefore.total.toString());

    // we first add a new candidate to ensure the nominator can request a revoke
    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1)
      .signAndSend(faithStash);

    await context.createBlock();

    // nominate faith to ensure ethan has a nomination
    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = rawTotalBefore.toJSON();

    // now we can schedule a revoke request
    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(alith.address)
      .signAndSend(ethan);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();
    // nomination should be set to zero
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(new BigNumber(0).toFixed());
    // total nomination should be decreased
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(stake.toFixed());

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    // a revoke request should be scheduled
    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(revoke.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(revoke.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(revoke.toFixed());
    expect(Object.keys(nominatorRequests.requests[alith.address].whenExecutable).length).equal(1);
    expect(nominatorRequests.requests[alith.address].action).equal('Revoke');

    // ethan removed
    // dorothy moved to top (with charleth)
    // empty bottom
    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap().toJSON();
    expect(topNominations.nominations.length).equal(2);
    expect(topNominations.nominations[0].owner.toString().toLowerCase()).equal(charleth.address.toLowerCase());
    expect(topNominations.nominations[1].owner.toString().toLowerCase()).equal(dorothy.address.toLowerCase());

    const rawBottomNominations: any = await context.polkadotApi.query.bfcStaking.bottomNominations(alith.address);
    const bottomNominations = rawBottomNominations.unwrap().toJSON();
    expect(bottomNominations.nominations.length).equal(0);

    // it should be moved to unstaking nominations
    const rawUnstakingNominations: any = await context.polkadotApi.query.bfcStaking.unstakingNominations(alith.address);
    const unstakingNominations = rawUnstakingNominations.unwrap().toJSON();
    expect(unstakingNominations.nominations.length).equal(1);
    expect(unstakingNominations.nominations[0].owner.toString().toLowerCase()).equal(ethan.address.toLowerCase());
    expect(new BigNumber(unstakingNominations.nominations[0].amount.toString()).toFixed()).equal(revoke.toFixed());

    // total should be decreased
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = rawTotalAfter.toJSON();
    expect(new BigNumber(totalAfter.toString()).toFixed()).equal(new BigNumber(totalBefore.toString()).minus(revoke).toFixed());
  });

  it('should successfully cancel a revoke request', async function () {
    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap().toJSON();
    const nominatorRequestsBefore = rawNominatorStateBefore.unwrap().requests.toJSON();
    const stakeBefore = new BigNumber(nominatorStateBefore.nominations[alith.address].toString());
    const totalNominationBefore = new BigNumber(nominatorStateBefore.total.toString());

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = new BigNumber(rawTotalBefore.toJSON().toString());

    const when = parseInt(Object.keys(nominatorRequestsBefore.requests[alith.address].whenExecutable)[0]);

    await context.polkadotApi.tx.bfcStaking
      .cancelNominationRequest(alith.address, when)
      .signAndSend(ethan);

    await context.createBlock();

    // ethan added to top (with charleth)
    // dorothy moved to bottom

    // top nominations should be rollbacked
    const rawTopNominationsAfter: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominationsAfter = rawTopNominationsAfter.unwrap().toJSON();
    expect(topNominationsAfter.nominations.length).equal(2);
    expect(topNominationsAfter.nominations[0].owner.toString().toLowerCase()).equal(ethan.address.toLowerCase());
    expect(topNominationsAfter.nominations[1].owner.toString().toLowerCase()).equal(charleth.address.toLowerCase());
    const cancelled = new BigNumber(topNominationsAfter.nominations[0].amount.toString());
    const lowestTopNomination = new BigNumber(topNominationsAfter.nominations[1].amount.toString());

    // bottom nominations should be rollbacked
    const rawBottomNominationsAfter: any = await context.polkadotApi.query.bfcStaking.bottomNominations(alith.address);
    const bottomNominationsAfter = rawBottomNominationsAfter.unwrap().toJSON();
    expect(bottomNominationsAfter.nominations.length).equal(1);
    expect(bottomNominationsAfter.nominations[0].owner.toString().toLowerCase()).equal(dorothy.address.toLowerCase());
    expect(new BigNumber(bottomNominationsAfter.total.toString()).toFixed()).equal(new BigNumber(bottomNominationsAfter.nominations[0].amount.toString()).toFixed());
    const highestBottomNomination = new BigNumber(bottomNominationsAfter.nominations[0].amount.toString());

    // nominator state should be rollbacked
    const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateAfter = rawNominatorStateAfter.unwrap().toJSON();
    const nominatorRequestsAfter = rawNominatorStateAfter.unwrap().requests.toJSON();
    expect(nominatorRequestsAfter.requests).is.empty;
    expect(new BigNumber(nominatorStateAfter.nominations[alith.address].toString()).toFixed()).equal(stakeBefore.plus(cancelled).toFixed());
    expect(new BigNumber(nominatorStateAfter.total.toString()).toFixed()).equal(totalNominationBefore.plus(cancelled).toFixed());

    // total should be rollbacked
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = rawTotalAfter.toJSON();
    expect(new BigNumber(totalAfter.toString()).toFixed()).equal(totalBefore.plus(cancelled).toFixed());

    // candidate state should be rollbacked
    const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateStateAfter = rawCandidateStateAfter.unwrap();
    const selfBondAfter = new BigNumber(candidateStateAfter.bond.toString());
    const votingPowerAfter = new BigNumber(candidateStateAfter.votingPower.toString());
    // voting power is the sum of self bond and top nominations
    expect(votingPowerAfter.toFixed()).equal(cancelled.plus(lowestTopNomination).plus(selfBondAfter).toFixed());
    expect(new BigNumber(candidateStateAfter.lowestTopNominationAmount.toString()).toFixed()).equal(lowestTopNomination.toFixed());
    expect(new BigNumber(candidateStateAfter.highestBottomNominationAmount.toString()).toFixed()).equal(highestBottomNomination.toFixed());
  });

  it('should successfully execute a revoke request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(alith.address)
      .signAndSend(ethan);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();
    const when = parseInt(Object.keys(nominatorRequests.requests[alith.address].whenExecutable)[0]);
    const revoked = new BigNumber(nominatorRequests.requests[alith.address].amount.toString());

    const accountBefore = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountBefore['data'].reserved.toString()).equal(revoked.plus(stake).toFixed());

    await jumpToRound(context, when);

    await context.polkadotApi.tx.bfcStaking
      .executeNominationRequest(alith.address, when)
      .signAndSend(ethan);

    await context.createBlock();

    // the reserved stake should be returned
    const accountAfter = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountAfter['data'].reserved.toString()).equal(new BigNumber(stake).toFixed());

    // the nomination should be revoked
    const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateAfter = rawNominatorStateAfter.unwrap().toJSON();
    expect(nominatorStateAfter.nominations[alith.address]).is.undefined;
  });

  it('should successfully schedule a leave request', async function () {
    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap();
    const stake = new BigNumber(nominatorStateBefore.total.toString());

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = rawTotalBefore.toJSON();

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(faith.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap();
    const votingPowerBefore = new BigNumber(candidateStateBefore.votingPower.toString());

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveNominators()
      .signAndSend(ethan);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();
    // nomination should be set to zero
    expect(new BigNumber(nominatorState.nominations[faith.address].toString()).toFixed()).equal(new BigNumber(0).toFixed());
    // total nomination should be set to zero
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(new BigNumber(0).toFixed());

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    // a leave request should be scheduled
    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[faith.address].amount.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[faith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(stake.toFixed());
    expect(Object.keys(nominatorRequests.requests[faith.address].whenExecutable).length).equal(1);
    expect(nominatorRequests.requests[faith.address].action).equal('Leave');

    // ethan removed
    // empty top
    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(faith.address);
    const topNominations = rawTopNominations.unwrap().toJSON();
    expect(topNominations.nominations.length).equal(0);

    // it should be moved to unstaking nominations
    const rawUnstakingNominations: any = await context.polkadotApi.query.bfcStaking.unstakingNominations(faith.address);
    const unstakingNominations = rawUnstakingNominations.unwrap().toJSON();
    expect(unstakingNominations.nominations.length).equal(1);
    expect(unstakingNominations.nominations[0].owner.toString().toLowerCase()).equal(ethan.address.toLowerCase());
    expect(new BigNumber(unstakingNominations.nominations[0].amount.toString()).toFixed()).equal(stake.toFixed());

    // total should be decreased
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = rawTotalAfter.toJSON();
    expect(new BigNumber(totalAfter.toString()).toFixed()).equal(new BigNumber(totalBefore.toString()).minus(stake).toFixed());

    // candidate state should be updated
    const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(faith.address);
    const candidateStateAfter = rawCandidateStateAfter.unwrap();
    let votingPowerAfter = new BigNumber(candidateStateAfter.votingPower.toString());
    expect(votingPowerAfter.toFixed()).equal(votingPowerBefore.minus(stake).toFixed());
    expect(new BigNumber(candidateStateAfter.lowestTopNominationAmount.toString()).toFixed()).equal(new BigNumber(0).toFixed());

    // candidate pool should be updated
    const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatePool = rawCandidatePool.toJSON();
    expect(new BigNumber(candidatePool[faith.address].toString()).toFixed()).equal(votingPowerAfter.toFixed());
  });

  it('should successfully bond more when leaving', async function () {
    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorRequestsBefore = rawNominatorStateBefore.unwrap().requests.toJSON();
    const stakeBefore = new BigNumber(nominatorRequestsBefore.lessTotal.toString());

    const more = new BigNumber(DEFAULT_STAKING_AMOUNT);

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = new BigNumber(rawTotalBefore.toJSON().toString());

    await context.polkadotApi.tx.bfcStaking
      .nominatorBondMore(faith.address, more.toFixed())
      .signAndSend(ethan);

    await context.createBlock();

    const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateAfter = rawNominatorStateAfter.unwrap().toJSON();
    const nominatorRequestsAfter = rawNominatorStateAfter.unwrap().requests.toJSON();
    const stakeAfter = new BigNumber(nominatorStateAfter.nominations[faith.address].toString());
    expect(stakeAfter.toFixed()).equal(stakeBefore.plus(more).toFixed()); // the nomination should be increased (with the more amount)
    expect(nominatorRequestsAfter.requests).is.empty; // the request should be cancelled

    // total should be increased (with cancelled amount)
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = new BigNumber(rawTotalAfter.toJSON().toString());
    expect(totalAfter.toFixed()).equal(totalBefore.plus(more).plus(stakeBefore).toFixed());
  });

  it('should successfully execute a leave request', async function () {
    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap().toJSON();
    const reservedBefore = new BigNumber(nominatorStateBefore.total.toString());

    const accountBefore = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountBefore['data'].reserved.toString()).equal(reservedBefore.toFixed());

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveNominators()
      .signAndSend(ethan);

    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    await context.polkadotApi.tx.bfcStaking
      .executeLeaveNominators(10)
      .signAndSend(ethan);

    await context.createBlock();

    // ethan removed
    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.toJSON();
    expect(nominatorState).is.null;

    // the reserved stake should be returned
    const accountAfter = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountAfter['data'].reserved.toString()).equal(new BigNumber(0).toFixed());
  });
});

describeDevNode('pallet_bfc_staking - candidate leave (while decreasing. last nomination)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const less = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1)
      .signAndSend(faithStash);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(faith.address, less.toFixed())
      .signAndSend(ethan);

    await context.createBlock();

    const account = await context.polkadotApi.query.system.account(ethan.address);
    expect(account['data'].reserved.toString()).equal(stake.toFixed());
  });

  it('should successfully schedule a candidate leave request', async function () {
    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(10)
      .signAndSend(faith);

    await context.createBlock();

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(faith.address);
    const candidateState = rawCandidateState.unwrap().toJSON();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    expect(candidateState.status.leaving).equal(currentRound + 1);
  });

  it('should successfully execute a candidate leave request', async function () {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap().toJSON();
    const stakeBefore = new BigNumber(nominatorStateBefore.nominations[faith.address].toString());

    const accountBefore = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountBefore['data'].reserved.toString()).equal(new BigNumber(DEFAULT_STAKING_AMOUNT).toFixed());

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(faith.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap();
    const selfBondBefore = new BigNumber(candidateStateBefore.bond.toString());

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = new BigNumber(rawTotalBefore.toJSON().toString());

    await context.polkadotApi.tx.bfcStaking
      .executeLeaveCandidates(10)
      .signAndSend(faithStash);

    await context.createBlock();

    const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(faith.address);
    const candidateStateAfter = rawCandidateStateAfter.toJSON();
    expect(candidateStateAfter).is.null;

    // the reserved stake should be returned
    const accountAfter = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountAfter['data'].reserved.toString()).equal(new BigNumber(0).toFixed());

    // nominator state should be removed (last nomination)
    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.toJSON();
    expect(nominatorState).is.null;

    // total should be decreased
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = new BigNumber(rawTotalAfter.toJSON().toString());
    expect(totalAfter.toFixed()).equal(totalBefore.minus(stakeBefore).minus(selfBondBefore).toFixed());
  });
});

describeDevNode('pallet_bfc_staking - candidate leave (while decreasing. not last nomination)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const less = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1)
      .signAndSend(faithStash);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(faith.address, less.toFixed())
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(10)
      .signAndSend(faith);

    await context.createBlock();
  });

  it('should successfully execute a candidate leave request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const less = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    const accountBefore = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountBefore['data'].reserved.toString()).equal(stake.multipliedBy(2).toFixed()); // faith + alith

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(faith.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap();
    const selfBondBefore = new BigNumber(candidateStateBefore.bond.toString());

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = new BigNumber(rawTotalBefore.toJSON().toString());

    await context.polkadotApi.tx.bfcStaking
      .executeLeaveCandidates(10)
      .signAndSend(faithStash);

    await context.createBlock();

    // the reserved stake should be returned
    const accountAfter = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountAfter['data'].reserved.toString()).equal(stake.toFixed());

    // nominator state should be updated
    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    expect(Object.keys(nominatorState.nominations).length).equal(1);
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stake.toFixed());

    // total should be decreased
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = new BigNumber(rawTotalAfter.toJSON().toString());
    expect(totalAfter.toFixed()).equal(totalBefore.minus(selfBondBefore).minus(stake.minus(less)).toFixed());
  });
});

describeDevNode('pallet_bfc_staking - candidate leave (while revoking)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1)
      .signAndSend(faithStash);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.multipliedBy(2).toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(dorothy);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(faith.address)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(10)
      .signAndSend(faith);

    await context.createBlock();
  });

  it('should successfully execute a candidate leave request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    const accountBefore = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountBefore['data'].reserved.toString()).equal(stake.multipliedBy(2).plus(stake).toFixed()); // faith + alith

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(faith.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap();
    const selfBondBefore = new BigNumber(candidateStateBefore.bond.toString());

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = new BigNumber(rawTotalBefore.toJSON().toString());

    await context.polkadotApi.tx.bfcStaking
      .executeLeaveCandidates(10)
      .signAndSend(faithStash);

    await context.createBlock();

    // the reserved stake should be returned
    const accountAfter = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountAfter['data'].reserved.toString()).equal(stake.toFixed()); // alith

    // nominator state should be updated
    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    expect(Object.keys(nominatorState.nominations).length).equal(1);
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stake.toFixed());

    // total should be decreased
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = new BigNumber(rawTotalAfter.toJSON().toString());
    expect(totalAfter.toFixed()).equal(totalBefore.minus(selfBondBefore).minus(stake).toFixed()); // decrease dorothy stake
  });
});

describeDevNode('pallet_bfc_staking - candidate leave (while leaving)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    // default Total = 1000 BFC

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1) // Total += 1000 BFC
      .signAndSend(faithStash);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.multipliedBy(2).toFixed(), 10, 10) // Total += 2000 BFC
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10) // Total += 1000 BFC
      .signAndSend(dorothy);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10) // Total += 1000 BFC
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveNominators() // Total -= (2000 + 1000) BFC
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(10)
      .signAndSend(faith);

    await context.createBlock();
  });

  it('should successfully execute a candidate leave request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    const accountBefore = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountBefore['data'].reserved.toString()).equal(stake.multipliedBy(2).plus(stake).toFixed()); // faith + alith

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(faith.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap();
    const selfBondBefore = new BigNumber(candidateStateBefore.bond.toString());

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = new BigNumber(rawTotalBefore.toJSON().toString());

    await context.polkadotApi.tx.bfcStaking
      .executeLeaveCandidates(10)
      .signAndSend(faithStash);

    await context.createBlock();

    // the reserved stake should be returned
    const accountAfter = await context.polkadotApi.query.system.account(ethan.address);
    expect(accountAfter['data'].reserved.toString()).equal(stake.toFixed()); // alith

    // nominator state should be updated
    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();
    expect(Object.keys(nominatorState.nominations).length).equal(1); // leave request for alith should be remained
    expect(Object.keys(nominatorRequests.requests).length).equal(1);
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(new BigNumber(0).toFixed());
    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(stake.toFixed());
    expect(Object.keys(nominatorRequests.requests[alith.address].whenExecutable).length).equal(1);
    expect(nominatorRequests.requests[alith.address].action).equal('Leave');

    // total should be decreased
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = new BigNumber(rawTotalAfter.toJSON().toString());
    expect(totalAfter.toFixed()).equal(totalBefore.minus(selfBondBefore).minus(stake).toFixed()); // decrease faith self-bond + dorothy stake
  });
});

describeDevNode('pallet_bfc_staking - cancel failures (revoke)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const lowStake = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC

    // default Total = 1000 BFC

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1)
      .signAndSend(faithStash);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, lowStake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(dorothy);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(charleth);

    await context.createBlock();
  });

  it('should fail to cancel revoke request due to below lowest bottom nomination', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(faith.address)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(baltathar);

    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await context.polkadotApi.tx.bfcStaking
      .cancelNominationRequest(faith.address, currentRound + 1)
      .signAndSend(ethan);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'cancelNominationRequest');
    expect(extrinsicResult).equal('CannotNominateLessThanLowestBottomWhenBottomIsFull');
  });
});

describeDevNode('pallet_bfc_staking - cancel failures (leave)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const lowStake = new BigNumber(DEFAULT_STAKING_AMOUNT).dividedBy(10); // 100 BFC

    // default Total = 1000 BFC

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1)
      .signAndSend(faithStash);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, lowStake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(dorothy);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(charleth);

    await context.createBlock();
  });

  it('should fail to cancel leave request due to below lowest bottom nomination', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveNominators()
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(baltathar);

    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await context.polkadotApi.tx.bfcStaking
      .cancelNominationRequest(faith.address, currentRound + 1)
      .signAndSend(ethan);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'cancelNominationRequest');
    expect(extrinsicResult).equal('CannotNominateLessThanLowestBottomWhenBottomIsFull');
  });
});

describeDevNode('pallet_bfc_staking - bond more with existing pending requests', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  // const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();
  });

  it('should successfully bond more with existing decrease request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const less = stake.dividedBy(10); // 100 BFC
    const more = less;

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(alith.address, less.toFixed())
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominatorBondMore(alith.address, more.toFixed())
      .signAndSend(ethan);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    // the decrease request should be remained
    expect(Object.keys(nominatorRequests.requests).length).equal(1);
    expect(nominatorRequests.requests[alith.address].action).equal('Decrease');
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(less.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(less.toFixed());
    expect(Object.keys(nominatorRequests.requests[alith.address].whenExecutable).length).equal(1);

    // nominator state should be updated
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(stake.toFixed());

    // candidate state should be updated
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();
    expect(candidateState.nominationCount.toString()).equal('1');
    expect(candidateState.lowestTopNominationAmount.toString()).equal(stake.toFixed());

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(stake);
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());

    // top nominations should be updated
    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap();
    expect(topNominations.nominations.length).equal(1);
    expect(topNominations.nominations[0].owner.toString().toLowerCase()).equal(ethan.address.toLowerCase());
    expect(topNominations.nominations[0].amount.toString()).equal(stake.toFixed());
  });

  it('should successfully bond more with existing revoke request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1)
      .signAndSend(faithStash);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(dorothy);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(dorothy);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(alith.address)
      .signAndSend(dorothy);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominatorBondMore(alith.address, stake.toFixed())
      .signAndSend(dorothy);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(dorothy.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();

    // the revoke request should be cancelled
    expect(Object.keys(nominatorRequests.requests).length).equal(0);

    // nominator state should be updated (bond more + cancelled amount)
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stake.plus(stake).toFixed());
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(stake.plus(stake.multipliedBy(2)).toFixed());

    // candidate state should be updated
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();
    expect(candidateState.nominationCount.toString()).equal('2');
    expect(candidateState.lowestTopNominationAmount.toString()).equal(stake.toFixed());

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(stake.plus(stake.multipliedBy(2)));
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());

    // top nominations should be updated
    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap();
    expect(topNominations.nominations.length).equal(2);
    expect(topNominations.nominations[0].owner.toString().toLowerCase()).equal(dorothy.address.toLowerCase());
    expect(topNominations.nominations[0].amount.toString()).equal(stake.plus(stake).toFixed());
  });

  it('should successfully bond more with existing leave request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(charleth);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(charleth);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveNominators()
      .signAndSend(charleth);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominatorBondMore(alith.address, stake.toFixed())
      .signAndSend(charleth);

    await context.createBlock();

    // the leave request should be cancelled
    // the remaining request should be switched to Revoke
    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    expect(Object.keys(nominatorRequests.requests).length).equal(1);
    expect(nominatorRequests.requests[faith.address].action).equal('Revoke');
    expect(new BigNumber(nominatorRequests.requests[faith.address].amount.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[faith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(stake.toFixed());
    expect(Object.keys(nominatorRequests.requests[faith.address].whenExecutable).length).equal(1);

    // nominator state should be updated
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(stake.plus(stake).toFixed());
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(stake.plus(stake).toFixed());

    // candidate state should be updated
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();
    expect(candidateState.nominationCount.toString()).equal('3');
    expect(candidateState.lowestTopNominationAmount.toString()).equal(stake.multipliedBy(2).toFixed());

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(stake.plus(stake.multipliedBy(3)));
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());
  });
});

describeDevNode('pallet_bfc_staking - revoke last nomination', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, null, stake.toFixed(), 1)
      .signAndSend(faithStash);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();
  });

  it('should successfully revoke last nomination', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = rawTotalBefore.toJSON();

    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(alith.address)
      .signAndSend(ethan);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();
    // nomination should be set to zero
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(new BigNumber(0).toFixed());
    // total nomination should be decreased
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(new BigNumber(0).toFixed());

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    // a revoke request should be scheduled
    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(stake.toFixed());
    expect(Object.keys(nominatorRequests.requests[alith.address].whenExecutable).length).equal(1);
    expect(nominatorRequests.requests[alith.address].action).equal('Revoke');

    // empty top and bottom nominations
    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap().toJSON();
    expect(topNominations.nominations.length).equal(0);
    const rawBottomNominations: any = await context.polkadotApi.query.bfcStaking.bottomNominations(alith.address);
    const bottomNominations = rawBottomNominations.unwrap().toJSON();
    expect(bottomNominations.nominations.length).equal(0);

    // it should be moved to unstaking nominations
    const rawUnstakingNominations: any = await context.polkadotApi.query.bfcStaking.unstakingNominations(alith.address);
    const unstakingNominations = rawUnstakingNominations.unwrap().toJSON();
    expect(unstakingNominations.nominations.length).equal(1);
    expect(unstakingNominations.nominations[0].owner.toString().toLowerCase()).equal(ethan.address.toLowerCase());
    expect(new BigNumber(unstakingNominations.nominations[0].amount.toString()).toFixed()).equal(stake.toFixed());

    // total should be decreased
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = rawTotalAfter.toJSON();
    expect(new BigNumber(totalAfter.toString()).toFixed()).equal(new BigNumber(totalBefore.toString()).minus(stake).toFixed());
  });

  it('should successfully revoke last nomination with existing revoke request', async function () {
    const stake = new BigNumber(DEFAULT_STAKING_AMOUNT);
    const totalRevoke = stake.multipliedBy(2);

    await context.polkadotApi.tx.bfcStaking
      .nominate(faith.address, stake.toFixed(), 10, 10)
      .signAndSend(ethan);

    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await context.polkadotApi.tx.bfcStaking
      .cancelNominationRequest(alith.address, currentRound + 1)
      .signAndSend(ethan);

    await context.createBlock();

    const rawTotalBefore: any = await context.polkadotApi.query.bfcStaking.total();
    const totalBefore = rawTotalBefore.toJSON();

    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(alith.address)
      .signAndSend(ethan);

    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(faith.address)
      .signAndSend(ethan);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(ethan.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();
    const nominatorRequests = rawNominatorState.unwrap().requests.toJSON();
    // nomination should be set to zero
    expect(new BigNumber(nominatorState.nominations[alith.address].toString()).toFixed()).equal(new BigNumber(0).toFixed());
    expect(new BigNumber(nominatorState.nominations[faith.address].toString()).toFixed()).equal(new BigNumber(0).toFixed());
    // total nomination should be decreased
    expect(new BigNumber(nominatorState.total.toString()).toFixed()).equal(new BigNumber(0).toFixed());

    // a revoke request should be scheduled
    expect(new BigNumber(nominatorRequests.lessTotal.toString()).toFixed()).equal(totalRevoke.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].amount.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[alith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(stake.toFixed());
    expect(Object.keys(nominatorRequests.requests[alith.address].whenExecutable).length).equal(1);
    expect(nominatorRequests.requests[alith.address].action).equal('Revoke');
    expect(new BigNumber(nominatorRequests.requests[faith.address].amount.toString()).toFixed()).equal(stake.toFixed());
    expect(new BigNumber(nominatorRequests.requests[faith.address].whenExecutable[currentRound + 1].toString()).toFixed()).equal(stake.toFixed());
    expect(Object.keys(nominatorRequests.requests[faith.address].whenExecutable).length).equal(1);
    expect(nominatorRequests.requests[faith.address].action).equal('Revoke');

    // empty top and bottom nominations
    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap().toJSON();
    expect(topNominations.nominations.length).equal(0);
    const rawBottomNominations: any = await context.polkadotApi.query.bfcStaking.bottomNominations(alith.address);
    const bottomNominations = rawBottomNominations.unwrap().toJSON();
    expect(bottomNominations.nominations.length).equal(0);

    // it should be moved to unstaking nominations
    const rawUnstakingNominations: any = await context.polkadotApi.query.bfcStaking.unstakingNominations(alith.address);
    const unstakingNominations = rawUnstakingNominations.unwrap().toJSON();
    expect(unstakingNominations.nominations.length).equal(1);
    expect(unstakingNominations.nominations[0].owner.toString().toLowerCase()).equal(ethan.address.toLowerCase());
    expect(new BigNumber(unstakingNominations.nominations[0].amount.toString()).toFixed()).equal(stake.toFixed());

    // total should be decreased
    const rawTotalAfter: any = await context.polkadotApi.query.bfcStaking.total();
    const totalAfter = rawTotalAfter.toJSON();
    expect(new BigNumber(totalAfter.toString()).toFixed()).equal(new BigNumber(totalBefore.toString()).minus(totalRevoke).toFixed());
  });
});
