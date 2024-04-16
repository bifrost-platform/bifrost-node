import BigNumber from 'bignumber.js';
import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import {
  AMOUNT_FACTOR, DEFAULT_STAKING_AMOUNT, MIN_BASIC_CANDIDATE_STAKING_AMOUNT,
  MIN_BASIC_VALIDATOR_STAKING_AMOUNT, MIN_FULL_CANDIDATE_STAKING_AMOUNT,
  MIN_FULL_VALIDATOR_STAKING_AMOUNT, MIN_NOMINATOR_STAKING_AMOUNT
} from '../../constants/currency';
import {
  SESSION_KEYS, TEST_CONTROLLERS, TEST_RELAYERS, TEST_STASHES
} from '../../constants/keys';
import { getExtrinsicResult, isEventTriggered } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';
import { jumpToRound } from '../utils';
// import { number } from 'yargs';

const DEFAULT_ROUND_LENGTH = 40;

describeDevNode('pallet_bfc_staking - set controller', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const newAlith = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const alithStash = keyring.addFromUri(TEST_STASHES[0].private);

  it('should successfully request controller address update', async function () {
    await context.polkadotApi.tx.bfcStaking
      .setController(newAlith.address)
      .signAndSend(alithStash);
    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    const rawControllerSets: any = await context.polkadotApi.query.bfcStaking.delayedControllerSets(currentRound);
    const controllerSets = rawControllerSets.toJSON();
    expect(controllerSets.length).equals(1);
  });

  it('should fail due to multiple requests', async function () {
    await context.polkadotApi.tx.bfcStaking
      .setController(newAlith.address)
      .signAndSend(alithStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setController');
    expect(extrinsicResult).equal('AlreadyControllerSetRequested');
  });

  it('should successfully replace controller account', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 0, 0)
      .signAndSend(charleth);
    await context.createBlock();

    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(alith.address)
      .signAndSend(charleth);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleRevokeNomination');
    expect(extrinsicResult).equal(null);

    let rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    let currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);

    const rawPrevCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const prevCandidateState = rawPrevCandidateState.toJSON();
    expect(prevCandidateState).to.be.null;

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(newAlith.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.stash).equal(alithStash.address);

    const rawBondedStash: any = await context.polkadotApi.query.bfcStaking.bondedStash(alithStash.address);
    const bondedStash = rawBondedStash.unwrap().toJSON();
    expect(bondedStash).equal(newAlith.address);

    const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatePool = rawCandidatePool.toJSON();
    let isCandidateFound = false;
    if (candidatePool[newAlith.address]) {
      isCandidateFound = true;
    }
    expect(isCandidateFound).equal(true);
    expect(Object.keys(candidatePool).length).equal(1);

    const rawSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const selectedCandidates = rawSelectedCandidates.toJSON();
    expect(selectedCandidates.length).equal(1)
    expect(selectedCandidates[0]).equal(newAlith.address);

    const rawSelectedFullCandidates: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    const selectedFullCandidates = rawSelectedFullCandidates.toJSON();
    expect(selectedFullCandidates.length).equal(1)
    expect(selectedFullCandidates[0]).equal(newAlith.address);

    const rawAtStake: any = await context.polkadotApi.query.bfcStaking.atStake(currentRound + 1, newAlith.address);
    const atStake = rawAtStake.toJSON();
    expect(atStake).is.not.null;

    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(newAlith.address);
    const topNominations = rawTopNominations.unwrap().toJSON();
    expect(topNominations.nominations.length).equal(1);
    let isTopNominationFound = false;
    for (const nomination of topNominations.nominations) {
      if (nomination.owner === charleth.address) {
        isTopNominationFound = true;
        break;
      }
    }
    expect(isTopNominationFound).equal(true);

    let rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    let nominatorState = rawNominatorState.unwrap().toJSON();

    // expect(nominatorState.nominations).has.key(newAlith.address);
    // expect(nominatorState.initialNominations).has.key(newAlith.address);

    expect(nominatorState.requests.revocationsCount).equal(1);
    expect(nominatorState.requests.requests).has.key(newAlith.address);
    expect(Object.keys(nominatorState.requests.requests).length).equal(1);
    expect(nominatorState.requests.requests[newAlith.address].validator).equal(newAlith.address);
  });
});

describeDevNode('pallet_bfc_staking - genesis', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithStash = keyring.addFromUri(TEST_STASHES[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  it('should match validator reserved bond from stash', async function () {
    const account = await context.polkadotApi.query.system.account(alithStash.address);
    expect(account['data'].reserved.toString()).equal(DEFAULT_STAKING_AMOUNT);
  });

  it('should include candidate to pool', async function () {
    const rawCandidates: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidates = rawCandidates.toJSON();
    expect(candidates).to.not.empty;
    expect(Object.keys(candidates).includes(alith.address));
  });

  it('should include validator as selected candidate', async function () {
    const rawSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const selectedCandidates = rawSelectedCandidates.toJSON();
    expect(selectedCandidates).to.not.be.empty;
    expect(selectedCandidates[0]).equal(alith.address);

    const rawSelectedFullCandidates: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    const selectedFullCandidates = rawSelectedFullCandidates.toJSON();
    expect(selectedFullCandidates).to.not.be.empty;
    expect(selectedFullCandidates[0]).equal(alith.address);

    const rawSelectedBasicCandidates: any = await context.polkadotApi.query.bfcStaking.selectedBasicCandidates();
    const selectedBasicCandidates = rawSelectedBasicCandidates.toJSON();
    expect(selectedBasicCandidates).to.be.empty;
  });

  it('should have correct candidate state information defined', async function () {
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap().toJSON();

    expect(candidateState).to.not.be.null;
    expect(candidateState).to.not.be.undefined;
    expect(candidateState.stash).equal(alithStash.address);
    expect(candidateState.tier).equal('Full');
  });

  it('should have bonded stash to controller', async function () {
    const rawBondedStash: any = await context.polkadotApi.query.bfcStaking.bondedStash(alithStash.address);
    const bondedStash = rawBondedStash.unwrap().toJSON();

    expect(bondedStash).to.not.be.null;
    expect(bondedStash).to.not.be.undefined;
    expect(bondedStash).equal(alith.address);
  });

  it('should include relayer to pool', async function () {
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();

    expect(relayerPool.length).equal(1);
    expect(relayerPool[0].relayer).equal(alithRelayer.address);
    expect(relayerPool[0].controller).equal(alith.address);
  });

  it('should include validator as selected relayer', async function () {
    const rawRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayers = rawRelayers.toJSON();

    expect(relayers.length).equal(1);
    expect(relayers[0]).equal(alithRelayer.address);
  });

  it('should have bonded controller to relayer', async function () {
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(alith.address);
    const bondedController = rawBondedController.unwrap().toJSON();

    expect(bondedController).to.not.be.null;
    expect(bondedController).to.not.be.undefined;
    expect(bondedController).equal(alithRelayer.address);
  });
});

describeDevNode('pallet_bfc_staking - staking inflations', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithStash = keyring.addFromUri(TEST_STASHES[0].private);

  it('should set inflation configurations on genesis', async function () {
    const rawInflation: any = await context.polkadotApi.query.bfcStaking.inflationConfig();
    const inflation = rawInflation.toJSON();

    expect(context.web3.utils.hexToNumberString(inflation.expect.min)).equal(new BigNumber(1000).multipliedBy(10 ** 18).toFixed());
    expect(context.web3.utils.hexToNumberString(inflation.expect.ideal)).equal(new BigNumber(2000).multipliedBy(10 ** 18).toFixed());
    expect(context.web3.utils.hexToNumberString(inflation.expect.max)).equal(new BigNumber(5000).multipliedBy(10 ** 18).toFixed());

    expect(inflation.annual.min.toString()).equal(new BigNumber(7).multipliedBy(10 ** 7).toFixed());
    expect(inflation.annual.ideal.toString()).equal(new BigNumber(13).multipliedBy(10 ** 7).toFixed());
    expect(inflation.annual.max.toString()).equal(new BigNumber(15).multipliedBy(10 ** 7).toFixed());
  });

  it('should fail to set staking expectations due to bad origin', async function () {
    await context.polkadotApi.tx.bfcStaking.setStakingExpectations({
      min: new BigNumber(2000).multipliedBy(10 ** 18).toFixed(),
      ideal: new BigNumber(3000).multipliedBy(10 ** 18).toFixed(),
      max: new BigNumber(4000).multipliedBy(10 ** 18).toFixed(),
    }).signAndSend(alithStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setStakingExpectations');
    expect(extrinsicResult).equal('BadOrigin');
  });

  it('should successfully set staking expectations', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setStakingExpectations({
        min: new BigNumber(2000).multipliedBy(10 ** 18).toFixed(),
        ideal: new BigNumber(3000).multipliedBy(10 ** 18).toFixed(),
        max: new BigNumber(4000).multipliedBy(10 ** 18).toFixed(),
      }),
    ).signAndSend(alith);
    await context.createBlock();

    const rawInflation: any = await context.polkadotApi.query.bfcStaking.inflationConfig();
    const inflation = rawInflation.toJSON();

    expect(context.web3.utils.hexToNumberString(inflation.expect.min)).equal(new BigNumber(2000).multipliedBy(10 ** 18).toFixed());
    expect(context.web3.utils.hexToNumberString(inflation.expect.ideal)).equal(new BigNumber(3000).multipliedBy(10 ** 18).toFixed());
    expect(context.web3.utils.hexToNumberString(inflation.expect.max)).equal(new BigNumber(4000).multipliedBy(10 ** 18).toFixed());
  });

  it('should fail to set staking inflation rate due to bad origin', async function () {
    await context.polkadotApi.tx.bfcStaking.setInflation({
      min: new BigNumber(10).multipliedBy(10 ** 7).toFixed(),
      ideal: new BigNumber(15).multipliedBy(10 ** 7).toFixed(),
      max: new BigNumber(20).multipliedBy(10 ** 7).toFixed(),
    }).signAndSend(alithStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setInflation');
    expect(extrinsicResult).equal('BadOrigin');
  });

  it('should successfully set staking inflation rate', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setInflation({
        min: new BigNumber(10).multipliedBy(10 ** 7).toFixed(),
        ideal: new BigNumber(15).multipliedBy(10 ** 7).toFixed(),
        max: new BigNumber(20).multipliedBy(10 ** 7).toFixed(),
      }),
    ).signAndSend(alith);
    await context.createBlock();

    const rawInflation: any = await context.polkadotApi.query.bfcStaking.inflationConfig();
    const inflation = rawInflation.toJSON();

    expect(inflation.annual.min.toString()).equal(new BigNumber(10).multipliedBy(10 ** 7).toFixed());
    expect(inflation.annual.ideal.toString()).equal(new BigNumber(15).multipliedBy(10 ** 7).toFixed());
    expect(inflation.annual.max.toString()).equal(new BigNumber(20).multipliedBy(10 ** 7).toFixed());
  });
});

describeDevNode('pallet_bfc_staking - round configuration', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithStash = keyring.addFromUri(TEST_STASHES[0].private);

  it('should fail to set round length due to bad origin', async function () {
    await context.polkadotApi.tx.bfcStaking
      .setBlocksPerRound(2000)
      .signAndSend(alithStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setBlocksPerRound');
    expect(extrinsicResult).equal('BadOrigin');
  });

  it('should successfully set new round length', async function () {
    const rawRound: any = await context.polkadotApi.query.bfcStaking.round();
    const round = rawRound.toJSON();
    expect(round.roundLength).equal(DEFAULT_ROUND_LENGTH);

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setBlocksPerRound(200),
    ).signAndSend(alith);
    await context.createBlock();

    const rawRoundV2: any = await context.polkadotApi.query.bfcStaking.round();
    const roundV2 = rawRoundV2.toJSON();
    expect(roundV2.roundLength).equal(200);
  });
});

describeDevNode('pallet_bfc_staking - validator tier', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithStash = keyring.addFromUri(TEST_STASHES[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);
  const baltatharRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(MIN_BASIC_CANDIDATE_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, null, stake.toFixed(), 1)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();
  });

  before('should successfully register session keys', async function () {
    // insert session key
    const keys: any = {
      aura: SESSION_KEYS[1].aura,
      grandpa: SESSION_KEYS[1].gran,
      imOnline: SESSION_KEYS[1].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  it('should fail to set tier due to same value', async function () {
    const more = new BigNumber(0);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorTier(more.toFixed(), 'Full', null)
      .signAndSend(alithStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setValidatorTier');
    expect(extrinsicResult).equal('NoWritingSameValue');
  });

  it('should fail to set tier due to non-stash account', async function () {
    const more = new BigNumber(0);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorTier(more.toFixed(), 'Basic', null)
      .signAndSend(alith);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setValidatorTier');
    expect(extrinsicResult).equal('StashDNE');
  });

  it('should fail to set tier due to relayer already joined', async function () {
    const more = new BigNumber(0);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorTier(more.toFixed(), 'Basic', alithRelayer.address)
      .signAndSend(alithStash);
    const block = await context.createBlock();

    const success = await isEventTriggered(
      context,
      block.block.hash,
      [
        { method: 'ExtrinsicFailed', section: 'system' },
      ],
    );
    expect(success).equal(true);
  });

  it('should fail to set tier due to stake below min', async function () {
    const more = new BigNumber(0);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorTier(more.toFixed(), 'Full', baltatharRelayer.address)
      .signAndSend(baltatharStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setValidatorTier');
    expect(extrinsicResult).equal('CandidateBondBelowMin');
  });

  it('should fail to set tier due to invalid tier', async function () {
    const more = new BigNumber(0);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorTier(more.toFixed(), 'Full', null)
      .signAndSend(baltatharStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setValidatorTier');
    expect(extrinsicResult).equal('InvalidTierType');
  });

  it('should successfully set tier - basic to full', async function () {
    const more = new BigNumber(MIN_FULL_VALIDATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorTier(more.toFixed(), 'Full', baltatharRelayer.address)
      .signAndSend(baltatharStash);
    await context.createBlock();

    // check candidate info
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.tier).equal('Full');

    // check relayer pool
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    expect(relayerPool.length).equal(2);
    expect(relayerPool[1].relayer).equal(baltatharRelayer.address);

    // check bonded controller
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(baltathar.address);
    const bondedController = rawBondedController.unwrap().toJSON();
    expect(bondedController).equal(baltatharRelayer.address);
  });

  it('should successfully set tier - full to basic', async function () {
    const more = new BigNumber(0);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorTier(more.toFixed(), 'Basic', null)
      .signAndSend(alithStash);
    await context.createBlock();

    // check candidate info
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.tier).equal('Basic');

    // check relayer pool
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    expect(relayerPool.length).equal(1);
    let isRelayerFound = false;
    for (const relayer of relayerPool) {
      if (relayer.relayer === alithRelayer.address) {
        isRelayerFound = true;
      }
    }
    expect(isRelayerFound).equal(false);

    // check bonded controller
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(alith.address);
    const bondedController = rawBondedController.toJSON();
    expect(bondedController).equal(null);
  });

  it('should successfully select correct candidates', async function () {
    this.timeout(20000);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);

    // check selected candidates
    const rawValidators: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const validators = rawValidators.toJSON();
    expect(validators.length).equal(2);

    expect(validators).includes(alith.address)
    expect(validators).includes(baltathar.address);

    // check selected full candidates
    const rawFullValidators: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    const fullValidators = rawFullValidators.toJSON();
    expect(fullValidators.length).equal(1);
    expect(fullValidators).includes(baltathar.address);

    // check selected basic candidates
    const rawBasicValidators: any = await context.polkadotApi.query.bfcStaking.selectedBasicCandidates();
    const basicValidators = rawBasicValidators.toJSON();
    expect(basicValidators.length).equal(1);
    expect(basicValidators).includes(alith.address);

    // check selected relayers
    const rawRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayers = rawRelayers.toJSON();
    expect(relayers.length).equal(1);
    expect(relayers).includes(baltatharRelayer.address);
  })
});

describeDevNode('pallet_bfc_staking - validator commission', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithStash = keyring.addFromUri(TEST_STASHES[0].private);

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(MIN_BASIC_CANDIDATE_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, null, stake.toFixed(), 1)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();
  });

  it('should fail to set global default commission rate due to bad origin', async function () {
    const commission = new BigNumber(70).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.bfcStaking
      .setDefaultValidatorCommission(commission.toFixed(), 'Full')
      .signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setDefaultValidatorCommission');
    expect(extrinsicResult).equal('BadOrigin');
  });

  it('should fail to set global default commission rate due to above max', async function () {
    const commission = new BigNumber(10).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setDefaultValidatorCommission(commission.toFixed(), 'Basic')
    ).signAndSend(alith);
    await context.createBlock();

    const rawCurrentCommission: any = await context.polkadotApi.query.bfcStaking.defaultFullValidatorCommission();
    const currentCommission = rawCurrentCommission.toNumber();
    const expectedCommission = new BigNumber(50).multipliedBy(10 ** 7);
    expect(currentCommission.toString()).equal(expectedCommission.toFixed());
  });

  it('should successfully update global default validator commission - full node', async function () {
    const commission = new BigNumber(70).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setDefaultValidatorCommission(commission.toFixed(), 'Full')
    ).signAndSend(alith);
    await context.createBlock();

    const rawCurrentCommission: any = await context.polkadotApi.query.bfcStaking.defaultFullValidatorCommission();
    const currentCommission = rawCurrentCommission.toNumber();
    expect(currentCommission.toString()).equal(commission.toFixed());
  });

  it('should successfully update global default validator commission - basic node', async function () {
    const commission = new BigNumber(10).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setDefaultValidatorCommission(commission.toFixed(), 'Basic')
    ).signAndSend(alith);
    await context.createBlock();

    const rawCurrentCommission: any = await context.polkadotApi.query.bfcStaking.defaultBasicValidatorCommission();
    const currentCommission = rawCurrentCommission.toNumber();
    expect(currentCommission.toString()).equal(commission.toFixed());
  });

  it('should successfully update global default validator commission - all', async function () {
    const commission = new BigNumber(20).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setDefaultValidatorCommission(commission.toFixed(), 'All')
    ).signAndSend(alith);
    await context.createBlock();

    const rawCurrentFullCommission: any = await context.polkadotApi.query.bfcStaking.defaultFullValidatorCommission();
    const currentFullCommission = rawCurrentFullCommission.toNumber();
    expect(currentFullCommission.toString()).equal(commission.toFixed());

    const rawCurrentBasicCommission: any = await context.polkadotApi.query.bfcStaking.defaultBasicValidatorCommission();
    const currentBasicCommission = rawCurrentBasicCommission.toNumber();
    expect(currentBasicCommission.toString()).equal(commission.toFixed());
  });

  it('should fail to set candidate commission rate due to invalid origin', async function () {
    const commission = new BigNumber(70).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorCommission(commission.toFixed())
      .signAndSend(alithStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setValidatorCommission');
    expect(extrinsicResult).equal('CandidateDNE');
  });

  it('should fail to set candidate commission rate due to same value', async function () {
    const commission = new BigNumber(50).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorCommission(commission.toFixed())
      .signAndSend(alith);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setValidatorCommission');
    expect(extrinsicResult).equal('NoWritingSameValue');
  });

  it('should fail to set candidate commission rate due to above max', async function () {
    const commission = new BigNumber(50).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.bfcStaking
      .setValidatorCommission(commission.toFixed())
      .signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'setValidatorCommission');
    expect(extrinsicResult).equal('CannotSetAboveMax');
  });

  it('should successfully request candidate validator commission rate update - full node', async function () {
    const commission = new BigNumber(70).multipliedBy(10 ** 7);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await context.polkadotApi.tx.bfcStaking
      .setValidatorCommission(commission.toFixed())
      .signAndSend(alith);
    await context.createBlock();

    const rawCommissionSets: any = await context.polkadotApi.query.bfcStaking.delayedCommissionSets(currentRound);
    const commissionSets = rawCommissionSets.toJSON();
    let isRequested = false;
    for (const set of commissionSets) {
      if (set.who === alith.address) {
        isRequested = true;
        expect(set.new.toString()).equal(commission.toFixed());
      }
    }
    expect(isRequested).equal(true);
  });

  it('should successfully request candidate validator commission rate update - basic node', async function () {
    const commission = new BigNumber(20).multipliedBy(10 ** 7);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await context.polkadotApi.tx.bfcStaking
      .setValidatorCommission(commission.toFixed())
      .signAndSend(baltathar);
    await context.createBlock();

    const rawCommissionSets: any = await context.polkadotApi.query.bfcStaking.delayedCommissionSets(currentRound);
    const commissionSets = rawCommissionSets.toJSON();
    let isRequested = false;
    for (const set of commissionSets) {
      if (set.who === baltathar.address) {
        isRequested = true;
        expect(set.new.toString()).equal(commission.toFixed());
      }
    }
    expect(isRequested).equal(true);
  });

  it('should successfully update requested commission updates in the next round', async function () {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    const commissionV1 = new BigNumber(70).multipliedBy(10 ** 7);
    const rawCandidateStateV1: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateStateV1 = rawCandidateStateV1.unwrap().toJSON();
    expect(candidateStateV1.commission.toString()).equal(commissionV1.toFixed());

    const commissionV2 = new BigNumber(20).multipliedBy(10 ** 7);
    const rawCandidateStateV2: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateStateV2 = rawCandidateStateV2.unwrap().toJSON();
    expect(candidateStateV2.commission.toString()).equal(commissionV2.toFixed());
  });

  it('should successfully update max validator commission - full node', async function () {
    const commission = new BigNumber(80).multipliedBy(10 ** 7);


    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setMaxValidatorCommission(commission.toFixed(), 'Full')
    ).signAndSend(alith);
    await context.createBlock();

    const rawCurrentCommission: any = await context.polkadotApi.query.bfcStaking.maxFullValidatorCommission();
    const currentCommission = rawCurrentCommission.toNumber();
    expect(currentCommission.toString()).equal(commission.toFixed());
  });

  it('should successfully update max validator commission - basic node', async function () {
    const commission = new BigNumber(50).multipliedBy(10 ** 7);

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setMaxValidatorCommission(commission.toFixed(), 'Basic')
    ).signAndSend(alith);
    await context.createBlock();

    const rawCurrentCommission: any = await context.polkadotApi.query.bfcStaking.maxBasicValidatorCommission();
    const currentCommission = rawCurrentCommission.toNumber();
    expect(currentCommission.toString()).equal(commission.toFixed());
  });
});

describeDevNode('pallet_bfc_staking - validator selection', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);

  it('should successfully update max full nodes selected', async function () {
    const max = 15;

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setMaxFullSelected(max)
    ).signAndSend(alith);
    await context.createBlock();

    const rawMaxFullSelected: any = await context.polkadotApi.query.bfcStaking.maxFullSelected();
    const maxFullSelected = rawMaxFullSelected.toNumber();
    expect(maxFullSelected).equal(max);

    const rawMaxBasicSelected: any = await context.polkadotApi.query.bfcStaking.maxBasicSelected();
    const maxBasicSelected = rawMaxBasicSelected.toNumber();
    const rawMaxTotalSelected: any = await context.polkadotApi.query.bfcStaking.maxTotalSelected();
    const maxTotalSelected = rawMaxTotalSelected.toNumber();
    expect(maxTotalSelected).equal(max + maxBasicSelected);
  });

  it('should successfully update max basic nodes selected', async function () {
    const max = 15;

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setMaxBasicSelected(max)
    ).signAndSend(alith);
    await context.createBlock();

    const rawMaxBasicSelected: any = await context.polkadotApi.query.bfcStaking.maxBasicSelected();
    const maxBasicSelected = rawMaxBasicSelected.toNumber();
    expect(maxBasicSelected).equal(max);

    const rawMaxFullSelected: any = await context.polkadotApi.query.bfcStaking.maxFullSelected();
    const maxFullSelected = rawMaxFullSelected.toNumber();
    const rawMaxTotalSelected: any = await context.polkadotApi.query.bfcStaking.maxTotalSelected();
    const maxTotalSelected = rawMaxTotalSelected.toNumber();
    expect(maxTotalSelected).equal(max + maxFullSelected);
  });

  it('should fail due to non-root origin', async function () {
    const max = 15;

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcStaking.setMaxFullSelected(max)
    ).signAndSend(baltathar);
    const block = await context.createBlock();

    const success = await isEventTriggered(
      context,
      block.block.hash,
      [
        { method: 'ExtrinsicFailed', section: 'system' },
      ],
    );
    expect(success).equal(true);
  });

  it('should successfully select top candidates', async function () {
    this.timeout(20000);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);
    await context.createBlock();

    const rawSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    expect(rawSelectedCandidates.toJSON().length).equal(1);

    const rawSelectedFullCandidates: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    expect(rawSelectedFullCandidates.toJSON().length).equal(1);

    const rawSelectedBasicCandidates: any = await context.polkadotApi.query.bfcStaking.selectedBasicCandidates();
    expect(rawSelectedBasicCandidates.toJSON().length).equal(0);
  });
});

describeDevNode('pallet_bfc_staking - join candidates', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithStash = keyring.addFromUri(TEST_STASHES[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);
  const baltatharRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const dorothyStash = keyring.addFromUri(TEST_STASHES[3].private);
  const dorothyRelayer = keyring.addFromUri(TEST_RELAYERS[3].private);

  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);
  const ethanStash = keyring.addFromUri(TEST_STASHES[4].private);

  const faith = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const faithStash = keyring.addFromUri(TEST_STASHES[5].private);
  const faithRelayer = keyring.addFromUri(TEST_RELAYERS[5].private);

  it('should fail due to stash already bonded', async function () {
    const stake = new BigNumber(MIN_BASIC_CANDIDATE_STAKING_AMOUNT);
    const candidateAmount = 1;

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(alith.address, null, stake.toFixed(), candidateAmount)
      .signAndSend(alithStash, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'joinCandidates');
    expect(extrinsicResult).equal('AlreadyBonded');
  });

  it('should fail due to controller already paired', async function () {
    const stake = new BigNumber(MIN_BASIC_CANDIDATE_STAKING_AMOUNT);
    const candidateAmount = 1;

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(alith.address, null, stake.toFixed(), candidateAmount)
      .signAndSend(ethanStash, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'joinCandidates');
    expect(extrinsicResult).equal('AlreadyPaired');
  });

  it('should fail due to relayer already bonded', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);
    const candidateAmount = 1;

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, alithRelayer.address, stake.toFixed(), candidateAmount)
      .signAndSend(baltatharStash, { nonce: -1 });
    const block = await context.createBlock();

    const success = await isEventTriggered(
      context,
      block.block.hash,
      [
        { method: 'ExtrinsicFailed', section: 'system' },
      ],
    );
    expect(success).equal(true);
  });

  it('should fail due to minimum amount constraint', async function () {
    const stakeBelowMin = new BigNumber(MIN_BASIC_CANDIDATE_STAKING_AMOUNT).minus(AMOUNT_FACTOR);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, null, stakeBelowMin.toFixed(), 1)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'joinCandidates');
    expect(extrinsicResult).equal('CandidateBondBelowMin');
  });

  it('should fail due to invalid candidate amount', async function () {
    const stake = new BigNumber(MIN_BASIC_CANDIDATE_STAKING_AMOUNT);
    const candidateAmount = 0;

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, null, stake.toFixed(), candidateAmount)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'joinCandidates');
    expect(extrinsicResult).equal('TooLowCandidateCountWeightHintJoinCandidates');
  });

  it('should successfully join candidate pool - full node', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);
    const rawCandidatesBefore: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesBefore = rawCandidatesBefore.toJSON();

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, baltatharRelayer.address, stake.toFixed(), Object.keys(candidatesBefore).length)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();

    const keys: any = {
      aura: SESSION_KEYS[1].aura,
      grandpa: SESSION_KEYS[1].gran,
      imOnline: SESSION_KEYS[1].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    // check candidate pool
    const rawCandidatesAfter: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesAfter = rawCandidatesAfter.toJSON();
    expect(Object.keys(candidatesAfter).length).equal(Object.keys(candidatesBefore).length + 1);
    expect(Object.keys(candidatesAfter)).includes(baltathar.address);

    // check candidate info
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.stash).equal(baltatharStash.address);
    expect(candidateState.tier).equal('Full');

    // check bonded stash
    const rawBondedStash: any = await context.polkadotApi.query.bfcStaking.bondedStash(baltatharStash.address);
    const bondedStash = rawBondedStash.unwrap().toJSON();
    expect(bondedStash).equal(baltathar.address);

    // check relayer pool
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    expect(relayerPool.length).equal(Object.keys(candidatesBefore).length + 1);
    expect(relayerPool[1].relayer).equal(baltatharRelayer.address);

    // check bonded controller
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(baltathar.address);
    const bondedController = rawBondedController.unwrap().toJSON();
    expect(bondedController).equal(baltatharRelayer.address);
  });

  it('should successfully join candidate pool with identical essential accounts - full node', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);
    const rawCandidatesBefore: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesBefore = rawCandidatesBefore.toJSON();

    // stash, controller, relayer with identical accounts
    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(charleth.address, charleth.address, stake.toFixed(), Object.keys(candidatesBefore).length)
      .signAndSend(charleth, { nonce: -1 });
    const keys: any = {
      aura: SESSION_KEYS[2].aura,
      grandpa: SESSION_KEYS[2].gran,
      imOnline: SESSION_KEYS[2].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(charleth, { nonce: -1 });
    await context.createBlock();

    // check candidate pool
    const rawCandidatesAfter: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesAfter = rawCandidatesAfter.toJSON();
    expect(Object.keys(candidatesAfter).length).equal(Object.keys(candidatesBefore).length + 1);
    expect(Object.keys(candidatesAfter)).includes(charleth.address);

    // check candidate info
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(charleth.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.stash).equal(charleth.address);
    expect(candidateState.tier).equal('Full');

    // check bonded stash
    const rawBondedStash: any = await context.polkadotApi.query.bfcStaking.bondedStash(charleth.address);
    const bondedStash = rawBondedStash.unwrap().toJSON();
    expect(bondedStash).equal(charleth.address);

    // check relayer pool
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    expect(relayerPool.length).equal(Object.keys(candidatesBefore).length + 1);
    expect(relayerPool[2].relayer).equal(charleth.address);

    // check bonded controller
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(charleth.address);
    const bondedController = rawBondedController.unwrap().toJSON();
    expect(bondedController).equal(charleth.address);
  });

  it('should successfully join candidate pool - basic node', async function () {
    const stake = new BigNumber(MIN_BASIC_CANDIDATE_STAKING_AMOUNT);
    const rawCandidatesBefore: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesBefore: any = rawCandidatesBefore.toJSON();

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(ethan.address, null, stake.toFixed(), Object.keys(candidatesBefore).length)
      .signAndSend(ethanStash, { nonce: -1 });
    const keys: any = {
      aura: SESSION_KEYS[4].aura,
      grandpa: SESSION_KEYS[4].gran,
      imOnline: SESSION_KEYS[4].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(ethan, { nonce: -1 });
    await context.createBlock();

    // check candidate pool
    const rawCandidatesAfter: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesAfter = rawCandidatesAfter.toJSON();
    expect(Object.keys(candidatesAfter).length).equal(Object.keys(candidatesBefore).length + 1);
    expect(Object.keys(candidatesAfter)).includes(ethan.address);

    // check candidate info
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(ethan.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.stash).equal(ethanStash.address);
    expect(candidateState.tier).equal('Basic');

    // check bonded stash
    const rawBondedStash: any = await context.polkadotApi.query.bfcStaking.bondedStash(ethanStash.address);
    const bondedStash = rawBondedStash.unwrap().toJSON();
    expect(bondedStash).equal(ethan.address);

    // check relayer pool
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    expect(relayerPool.length).equal(Object.keys(candidatesBefore).length);

    // check bonded controller
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(ethan.address);
    const bondedController = rawBondedController.toJSON();
    expect(bondedController).to.be.null;
  });

  it('should fail to be a validator due to minimum validator stake constraint', async function () {
    this.timeout(20000);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);

    const validators: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    expect(Object.keys(validators.toJSON()).length).equal(1);
  });

  it('should successfully join candidate pool and be selected as a validator in the next round - full node', async function () {
    this.timeout(20000);

    const stake = new BigNumber(MIN_FULL_VALIDATOR_STAKING_AMOUNT);
    const candidatesBefore: any = await context.polkadotApi.query.bfcStaking.candidatePool();

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(faith.address, faithRelayer.address, stake.toFixed(), Object.keys(candidatesBefore).length)
      .signAndSend(faithStash, { nonce: -1 });
    const keys: any = {
      aura: SESSION_KEYS[5].aura,
      grandpa: SESSION_KEYS[5].gran,
      imOnline: SESSION_KEYS[5].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(faith, { nonce: -1 });
    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);

    // check selected candidates
    const rawValidators: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const validators = rawValidators.toJSON();
    expect(validators.length).equal(2);

    expect(validators).includes(alith.address)
    expect(validators).includes(faith.address);

    // check selected full candidates
    const rawFullValidators: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    const fullValidators = rawFullValidators.toJSON();
    expect(fullValidators.length).equal(2);

    expect(fullValidators).includes(alith.address);
    expect(fullValidators).includes(faith.address);

    // check selected basic candidates
    const rawBasicValidators: any = await context.polkadotApi.query.bfcStaking.selectedBasicCandidates();
    const basicValidators = rawBasicValidators.toJSON();
    expect(basicValidators).to.be.empty;

    // check selected relayers
    const rawRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayers = rawRelayers.toJSON();
    expect(relayers.length).equal(2);
    expect(relayers).includes(alithRelayer.address);
    expect(relayers).includes(faithRelayer.address);
  });

  it('should successfully join candidate pool and be selected as a validator in the next round - basic node', async function () {
    this.timeout(20000);

    const stake = new BigNumber(MIN_BASIC_VALIDATOR_STAKING_AMOUNT);
    const rawCandidatesBefore: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesBefore = rawCandidatesBefore.toJSON();

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(dorothy.address, null, stake.toFixed(), Object.keys(candidatesBefore).length)
      .signAndSend(dorothyStash, { nonce: -1 });
    const keys: any = {
      aura: SESSION_KEYS[3].aura,
      grandpa: SESSION_KEYS[3].gran,
      imOnline: SESSION_KEYS[3].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(dorothy, { nonce: -1 });
    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);

    // check selected candidates
    const rawValidators: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const validators = rawValidators.toJSON();
    expect(validators.length).equal(3);

    expect(validators).includes(alith.address);
    expect(validators).includes(faith.address);
    expect(validators).includes(dorothy.address);

    // check selected full candidates
    const rawFullValidators: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    const fullValidators = rawFullValidators.toJSON();
    expect(fullValidators.length).equal(2);

    expect(fullValidators).includes(alith.address);
    expect(fullValidators).includes(faith.address);
    expect(fullValidators).not.includes(dorothyRelayer.address);

    // check selected basic candidates
    const rawBasicValidators: any = await context.polkadotApi.query.bfcStaking.selectedBasicCandidates();
    const basicValidators = rawBasicValidators.toJSON();
    expect(basicValidators.length).equal(1);
    expect(basicValidators).includes(dorothy.address);

    // check selected relayers - not included due to basic node
    const rawRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayers = rawRelayers.toJSON();
    expect(relayers.length).equal(2);
    expect(relayers).includes(alithRelayer.address);
    expect(relayers).includes(faithRelayer.address);
    expect(relayers).not.includes(dorothyRelayer.address);
  });
});

describeDevNode('pallet_bfc_staking - session keys', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);
  const baltatharRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(MIN_FULL_VALIDATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, baltatharRelayer.address, stake.toFixed(), 1)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();
  });

  it('should not be selected due to empty session keys', async function () {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);

    const rawValidators: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const validators = rawValidators.toJSON();
    expect(validators.length).equal(1);
    expect(validators).not.include(baltathar.address);
  });

  it('should be selected due to existing session keys', async function () {
    const keys: any = {
      aura: SESSION_KEYS[1].aura,
      grandpa: SESSION_KEYS[1].gran,
      imOnline: SESSION_KEYS[1].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    const rawValidators: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const validators = rawValidators.toJSON();
    expect(validators.length).equal(2);
    expect(validators).include(baltathar.address);
  });
});

describeDevNode('pallet_bfc_staking - candidate stake management', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);
  const baltatharRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const charlethStash = keyring.addFromUri(TEST_STASHES[2].private);
  const charlethRelayer = keyring.addFromUri(TEST_RELAYERS[2].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, baltatharRelayer.address, stake.toFixed(), 1)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();
  });

  before('should successfully register session keys', async function () {
    const keys: any = {
      aura: SESSION_KEYS[1].aura,
      grandpa: SESSION_KEYS[1].gran,
      imOnline: SESSION_KEYS[1].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  it('should fail due to non-stash origin', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .candidateBondMore(stake.toFixed())
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'candidateBondMore');
    expect(extrinsicResult).equal('StashDNE');
  });

  it('should successfully self bond more stake', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .candidateBondMore(stake.toFixed())
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap();
    expect(candidateState.bond.toString()).equal(stake.multipliedBy(2).toFixed());
  });

  it('should fail due to minimum candidate stake constraint', async function () {
    const candidateBondLess = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT).plus(AMOUNT_FACTOR);

    await context.polkadotApi.tx.bfcStaking
      .scheduleCandidateBondLess(candidateBondLess.toFixed())
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleCandidateBondLess');
    expect(extrinsicResult).equal('CandidateBondBelowMin');
  });

  it('should fail due to non-controller origin', async function () {
    const stake = new BigNumber(AMOUNT_FACTOR);

    await context.polkadotApi.tx.bfcStaking
      .scheduleCandidateBondLess(stake.toFixed())
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleCandidateBondLess');
    expect(extrinsicResult).equal('CandidateDNE');
  });

  it('should successfully schedule candidate bond less', async function () {
    const stake = new BigNumber(AMOUNT_FACTOR);

    await context.polkadotApi.tx.bfcStaking
      .scheduleCandidateBondLess(stake.toFixed())
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(context.web3.utils.hexToNumberString(candidateState.request.amount)).equal(stake.toFixed());
  });

  it('should fail due to pending schedule existance', async function () {
    const candidateBondLess = new BigNumber(AMOUNT_FACTOR);

    // multiple scheduled requests is not allowed
    await context.polkadotApi.tx.bfcStaking
      .scheduleCandidateBondLess(candidateBondLess.toFixed())
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleCandidateBondLess');
    expect(extrinsicResult).equal('PendingCandidateRequestAlreadyExists');
  });

  it('should fail due to wrong round to execute schedule candidate bond less', async function () {
    await context.polkadotApi.tx.bfcStaking
      .executeCandidateBondLess()
      .signAndSend(baltatharStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeCandidateBondLess');
    expect(extrinsicResult).equal('PendingCandidateRequestNotDueYet');
  });

  it('should fail due to non-stash origin', async function () {
    await context.polkadotApi.tx.bfcStaking
      .executeCandidateBondLess()
      .signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeCandidateBondLess');
    expect(extrinsicResult).equal('StashDNE');
  });

  it('should successfully execute candidate bond less', async function () {
    this.timeout(20000);

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap();

    const executableRound = Number(candidateStateBefore.request.toHuman()['whenExecutable']);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    const rawRoundDelay: any = context.polkadotApi.consts.bfcStaking.candidateBondLessDelay;
    const roundDelay = rawRoundDelay.toNumber();

    expect(executableRound).equal(currentRound + roundDelay);

    await jumpToRound(context, executableRound);

    await context.polkadotApi.tx.bfcStaking
      .executeCandidateBondLess()
      .signAndSend(baltatharStash);
    await context.createBlock();
    await context.createBlock();

    const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateStateAfter = rawCandidateStateAfter.unwrap().toJSON();

    const expectedBond = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT).multipliedBy(2).minus(AMOUNT_FACTOR);
    expect(candidateStateAfter.request).to.be.null;
    expect(new BigNumber(candidateStateAfter.bond).eq(expectedBond)).equal(true);
  });

  it('should successfully cancel scheduled candidate bond less', async function () {
    this.timeout(20000);

    const candidateBondLess = new BigNumber(AMOUNT_FACTOR);

    await context.polkadotApi.tx.bfcStaking
      .scheduleCandidateBondLess(candidateBondLess.toFixed())
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap().toJSON();

    const executableRound = Number(candidateStateBefore.request.whenExecutable);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    const rawRoundDelay: any = context.polkadotApi.consts.bfcStaking.candidateBondLessDelay;
    const roundDelay = rawRoundDelay.toNumber();

    expect(executableRound).equal(currentRound + roundDelay);

    await jumpToRound(context, executableRound);

    await context.polkadotApi.tx.bfcStaking
      .cancelCandidateBondLess()
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
    await context.createBlock();

    const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateStateAfter = rawCandidateStateAfter.unwrap().toJSON();
    expect(candidateStateAfter.request).to.be.null;
  });

  it('should be non-selected as active validator at next round - full node', async function () {
    this.timeout(20000);
    const stake = new BigNumber(MIN_FULL_VALIDATOR_STAKING_AMOUNT);

    // 1. join candidates
    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(charleth.address, charlethRelayer.address, stake.toFixed(), 10)
      .signAndSend(charlethStash, { nonce: -1 });
    await context.createBlock();

    const keys: any = {
      aura: SESSION_KEYS[2].aura,
      grandpa: SESSION_KEYS[2].gran,
      imOnline: SESSION_KEYS[2].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(charleth, { nonce: -1 });
    await context.createBlock();

    // 2. jump to next round
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    // 3. check selected candidates
    const rawValidators: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const validators = rawValidators.toJSON();
    expect(validators).includes(charleth.address);

    // 4. check selected full candidates
    const rawFullValidators: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    const fullValidators = rawFullValidators.toJSON();
    expect(fullValidators).includes(charleth.address);

    // 5. check selected relayers
    const rawRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayers = rawRelayers.toJSON();
    expect(relayers).includes(charlethRelayer.address);

    // 6. schedule bond less
    const less = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT).minus(stake).abs();
    await context.polkadotApi.tx.bfcStaking
      .scheduleCandidateBondLess(less.toFixed())
      .signAndSend(charleth, { nonce: -1 });
    await context.createBlock();

    // 7. jump to executable round
    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(charleth.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap().toJSON();
    const executableRound = Number(candidateStateBefore.request.whenExecutable);
    await jumpToRound(context, executableRound);

    // 8. execute bond less
    await context.polkadotApi.tx.bfcStaking
      .executeCandidateBondLess()
      .signAndSend(charlethStash);
    await context.createBlock();

    // 9. jump to next round
    const rawCurrentRoundV2: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRoundV2 = rawCurrentRoundV2.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRoundV2 + 1);

    // 10. check selected candidates
    const rawValidatorsV2: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const validatorsV2 = rawValidatorsV2.toJSON();
    expect(validatorsV2).not.includes(charleth.address);

    // 11. check selected full candidates
    const rawFullValidatorsV2: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    const fullValidatorsV2 = rawFullValidatorsV2.toJSON();
    expect(fullValidatorsV2).not.includes(charleth.address);

    // 12. check selected relayers
    const rawRelayersV2: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayersV2 = rawRelayersV2.toJSON();
    expect(relayersV2).not.includes(charlethRelayer.address);
  });
});

describeDevNode('pallet_bfc_staking - candidate leave', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);
  const baltatharRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  before('should successfully join candidate pool', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, baltatharRelayer.address, stake.toFixed(), 1)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();
  });

  before('should successfully register session keys', async function () {
    const keys: any = {
      aura: SESSION_KEYS[1].aura,
      grandpa: SESSION_KEYS[1].gran,
      imOnline: SESSION_KEYS[1].aura,
    };
    await context.polkadotApi.tx.session
      .setKeys(keys, '0x00')
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  it('should fail due to non-controller origin', async function () {
    const candidates: any = await context.polkadotApi.query.bfcStaking.candidatePool();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(candidates.length)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleLeaveCandidates');
    expect(extrinsicResult).equal('CandidateDNE');
  });

  it('should fail due to existing controller address update request', async function () {
    await context.polkadotApi.tx.bfcStaking
      .setController(charleth.address)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();
    await context.createBlock();

    const candidates: any = await context.polkadotApi.query.bfcStaking.candidatePool();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(candidates.length)
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleLeaveCandidates');
    expect(extrinsicResult).equal('CannotLeaveIfControllerSetRequested');

    // cancel request for continueing tests
    await context.polkadotApi.tx.bfcStaking
      .cancelControllerSet()
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  it('should fail due to existing commission update request', async function () {
    const commission = new BigNumber(70).multipliedBy(10 ** 7);
    await context.polkadotApi.tx.bfcStaking
      .setValidatorCommission(commission.toFixed())
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const candidates: any = await context.polkadotApi.query.bfcStaking.candidatePool();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(candidates.length)
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleLeaveCandidates');
    expect(extrinsicResult).equal('CannotLeaveIfCommissionSetRequested');

    // cancel request for continueing tests
    await context.polkadotApi.tx.bfcStaking
      .cancelValidatorCommissionSet()
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  it('should fail due to invalid candidate', async function () {
    const candidates: any = await context.polkadotApi.query.bfcStaking.candidatePool();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(candidates.length)
      .signAndSend(charleth, { nonce: -1 });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleLeaveCandidates');
    expect(extrinsicResult).equal('CandidateDNE');
  });

  it('should fail due to invalid candidate count', async function () {
    const candidates = 0;

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(candidates)
      .signAndSend(baltathar, { nonce: -1 });

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleLeaveCandidates');
    expect(extrinsicResult).equal('TooLowCandidateCountToLeaveCandidates');
  });

  it('should successfully schedule leave candidates', async function () {
    const rawCandidatesBefore: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesBefore = await rawCandidatesBefore.toJSON();

    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(Object.keys(candidatesBefore).length)
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const rawRoundDelay: any = context.polkadotApi.consts.bfcStaking.leaveCandidatesDelay;
    const roundDelay = rawRoundDelay.toNumber();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap();

    expect(candidateState.status.isLeaving).equal(true);
    expect(candidateState.status.asLeaving.toNumber()).equal(currentRound + roundDelay);
  });

  it('should fail due to wrong round to execute schedule leave candidates', async function () {
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap();

    await context.polkadotApi.tx.bfcStaking
      .executeLeaveCandidates(candidateState.nominationCount)
      .signAndSend(baltatharStash);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeLeaveCandidates');
    expect(extrinsicResult).equal('CandidateCannotLeaveYet');
  });

  it('should successfully cancel schedule leave candidates', async function () {
    const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();

    await context.polkadotApi.tx.bfcStaking
      .cancelLeaveCandidates(rawCandidatePool.length)
      .signAndSend(baltathar);
    await context.createBlock();
  });

  it('should successfully execute scheduled leave candidates', async function () {
    this.timeout(20000);

    // 1. schedule leave candidates
    const rawCandidatesBefore: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatesBefore = rawCandidatesBefore.toJSON();
    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveCandidates(Object.keys(candidatesBefore).length)
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    // 2. jump to executable round
    const balanceBefore = new BigNumber((await context.web3.eth.getBalance(baltatharStash.address)).toString());

    const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateStateBefore = rawCandidateStateBefore.unwrap();
    const candidateStatusBefore = candidateStateBefore.status.toHuman();

    const executableRound = Number(candidateStatusBefore['Leaving']);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    const rawRoundDelay: any = context.polkadotApi.consts.bfcStaking.leaveCandidatesDelay;
    const roundDelay = rawRoundDelay.toNumber();

    expect(executableRound).equal(currentRound + roundDelay);
    await jumpToRound(context, executableRound);

    // 3. execute leave candidates
    await context.polkadotApi.tx.bfcStaking
      .executeLeaveCandidates(Number(candidateStateBefore.nominationCount))
      .signAndSend(baltatharStash);
    await context.createBlock();
    await context.createBlock();

    // 4. check candidate information - must be null
    const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateStateAfter = rawCandidateStateAfter.toJSON();
    expect(candidateStateAfter).to.be.null;

    // 5. check candidate pool - must be not found
    const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatePool = rawCandidatePool.toJSON();
    expect(candidatePool).not.has.key(baltathar.address);

    // 6. check balance - self-bond must be unreserved
    const balanceAfter = new BigNumber((await context.web3.eth.getBalance(baltatharStash.address)).toString());
    expect(balanceAfter.isGreaterThan(balanceBefore)).equal(true);

    // 7. jump to next round
    const rawCurrentRoundV2: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRoundV2 = rawCurrentRoundV2.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRoundV2 + 1);

    // 8. check selected candidates
    const rawValidatorsV2: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const validatorsV2 = rawValidatorsV2.toJSON();
    expect(validatorsV2).not.includes(baltathar.address);

    // 9. check selected full candidates
    const rawFullValidators: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
    const fullValidators = rawFullValidators.toJSON();
    expect(fullValidators).not.includes(baltathar.address);

    // 10. check selected relayers
    const rawRelayersV2: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayersV2 = rawRelayersV2.toJSON();
    expect(relayersV2).not.includes(baltatharRelayer.address);

    // 11. check bonded controller
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(baltathar.address);
    const bondedController = rawBondedController.toJSON();
    expect(bondedController).to.be.null;

    // 12. check relayer pool
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    let isRelayerFound = false;
    for (const relayer of relayerPool) {
      if (relayer.controller === baltathar.address) {
        isRelayerFound = true;
        break;
      }
    }
    expect(isRelayerFound).equal(false);
  });
});

describeDevNode('pallet_bfc_staking - join nominators', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  it('should fail due to minimum amount constraint', async function () {
    const stakeBelowMin = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT).minus(10 ** 15);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stakeBelowMin.toFixed(), 0, 0)
      .signAndSend(charleth);

    await context.createBlock();
  });

  it('should fail due to wrong candidate', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(charleth.address, stake.toFixed(), 0, 0)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
    expect(extrinsicResult).equal('CandidateDNE');
  });

  it('should successfully nominate to alith', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 0, 0)
      .signAndSend(charleth);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();

    expect(nominatorState.nominations).has.key(alith.address);
    expect(parseInt(nominatorState.nominations[alith.address].toString(), 16).toString()).equal(stake.toFixed());

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();

    expect(candidateState.nominationCount.toString()).equal('1');

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(stake);
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());

    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap();

    expect(topNominations.nominations[0].owner.toString().toLowerCase()).equal(charleth.address.toLowerCase());
    expect(topNominations.nominations[0].amount.toString()).equal(stake.toFixed());
  });

  it('should fail due to calling nominate function twice', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 1, 1)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
    expect(extrinsicResult).equal('AlreadyNominatedCandidate');
  });
});

describeDevNode('pallet_bfc_staking - nominator stake management', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  before('should successfully nominate to alith', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 0, 0)
      .signAndSend(charleth);

    await context.createBlock();
  });

  it('should fail due to calling nominatorBondMore before nominate', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominatorBondMore(baltathar.address, stake.toFixed())
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominatorBondMore');
    expect(extrinsicResult).equal('NominationDNE');
  });

  it('should successfully request nominator bond more', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominatorBondMore(alith.address, stake.toFixed())
      .signAndSend(charleth);

    await context.createBlock();

    const stakeAfter = stake.multipliedBy(2);

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();

    expect(nominatorState.nominations).has.key(alith.address);
    expect(parseInt(nominatorState.nominations[alith.address].toString(), 16).toString()).equal(stakeAfter.toFixed());

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap();

    expect(candidateState.nominationCount.toString()).equal('1');

    const selfBond = new BigNumber(candidateState.bond.toString());
    const expectedStake = selfBond.plus(stakeAfter);
    expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());

    const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
    const topNominations = rawTopNominations.unwrap();

    expect(topNominations.nominations[0].owner.toString().toLowerCase()).equal(charleth.address.toLowerCase());
    expect(topNominations.nominations[0].amount.toString()).equal(stakeAfter.toFixed());
  });

  it('should fail due to minimum nominator stake constraint', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT).plus(AMOUNT_FACTOR);

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorState = rawNominatorState.unwrap().toJSON();

    expect(nominatorState.nominations).has.key(alith.address);
    expect(parseInt(nominatorState.nominations[alith.address].toString(), 16).toString()).equal(stake.toFixed());

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(alith.address, stake.toFixed())
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
    expect(extrinsicResult).equal('NominationBelowMin');
  });

  it('should successfully schedule nominator bond less', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .scheduleNominatorBondLess(alith.address, stake.toFixed())
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
    expect(extrinsicResult).equal(null);

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorState = rawNominatorState.unwrap();
    const nominatorRequests = nominatorState.requests.toJSON();

    expect(nominatorRequests.requests[alith.address].validator).equal(alith.address);
    expect(context.web3.utils.hexToNumberString(nominatorRequests.requests[alith.address].amount)).equal(stake.toFixed());

    let validator = null;
    let amount = null;
    let whenExecutable = null;
    let action = null;

    Object.values(nominatorRequests['requests']).forEach(function (value: any) {
      validator = value.validator;
      amount = context.web3.utils.hexToNumberString(value.amount);
      whenExecutable = value.whenExecutable;
      action = value.action;
    });

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    const rawRoundDelay: any = context.polkadotApi.consts.bfcStaking.nominationBondLessDelay;
    const roundDelay = rawRoundDelay.toNumber();

    expect(validator).equal(alith.address);
    expect(amount).equal(stake.toFixed());
    expect(whenExecutable).equal(currentRound + roundDelay);
    expect(action).equal('Decrease');
  });

  it('should fail due to wrong round to execute schedule nominator bond less', async function () {
    await context.polkadotApi.tx.bfcStaking
      .executeNominationRequest(alith.address)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeNominationRequest');
    expect(extrinsicResult).equal('PendingNominationRequestNotDueYet');
  });

  it('should successfully execute scheduled nominator bond less', async function () {
    this.timeout(20000);

    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap();
    const nominatorRequestsBefore = nominatorStateBefore.requests.toJSON();

    let validator = null;
    Object.keys(nominatorRequestsBefore['requests']).forEach(function (key) {
      validator = key.toLowerCase();
    });

    expect(validator).equal(alith.address.toLowerCase());

    let whenExecutable = null;
    Object.values(nominatorRequestsBefore['requests']).forEach(function (value: any) {
      whenExecutable = value.whenExecutable;
    });
    expect(whenExecutable).to.be.not.null;

    await jumpToRound(context, Number(whenExecutable));

    await context.polkadotApi.tx.bfcStaking
      .executeNominationRequest(alith.address)
      .signAndSend(charleth);

    await context.createBlock();
    await context.createBlock();

    const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorStateAfter = rawNominatorStateAfter.unwrap().toJSON();
    const nominatorRequestsAfter = nominatorStateAfter.requests;

    let validatorAfter = null;
    Object.keys(nominatorRequestsAfter['requests']).forEach(function (key) {
      validator = key.toLowerCase();
    });
    expect(validatorAfter).to.be.null;
    expect(parseInt(nominatorStateAfter.nominations[alith.address].toString(), 16).toString()).equal(MIN_NOMINATOR_STAKING_AMOUNT);
  });
});

describeDevNode('pallet_bfc_staking - revoke nomination', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  before('should successfully nominate to alith', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 0, 0)
      .signAndSend(charleth);

    await context.createBlock();
  });

  // even though candidate does not exist in pool
  // the returned error will be NominationDNE
  it('should fail due to nomination not found', async function () {
    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(baltathar.address)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleRevokeNomination');
    expect(extrinsicResult).equal('NominatorDNE');
  });

  it('should successfully schedule revoke nomination', async function () {
    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(alith.address)
      .signAndSend(charleth);

    await context.createBlock();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorState = rawNominatorState.unwrap();
    const nominatorRequests = nominatorState.requests.toJSON();

    expect(nominatorRequests['revocationsCount']).equal(1);

    let validator = null;
    Object.keys(nominatorRequests['requests']).forEach(function (key) {
      validator = key.toLowerCase();
    });

    expect(validator).equal(alith.address.toLowerCase());

    let amount = null;
    let whenExecutable = null;
    let action = null;
    Object.values(nominatorRequests['requests']).forEach(function (value: any) {
      amount = context.web3.utils.hexToNumberString(value.amount);
      whenExecutable = value.whenExecutable;
      action = value.action;
    });

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    const rawRoundDelay: any = context.polkadotApi.consts.bfcStaking.nominationBondLessDelay;
    const roundDelay = rawRoundDelay.toNumber();

    expect(amount).equal(MIN_NOMINATOR_STAKING_AMOUNT);
    expect(whenExecutable).equal(currentRound + roundDelay);
    expect(action).equal('Revoke');
  });

  it('should fail due to duplicate requests', async function () {
    await context.polkadotApi.tx.bfcStaking
      .scheduleRevokeNomination(alith.address)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleRevokeNomination');
    expect(extrinsicResult).equal('PendingNominationRequestAlreadyExists');
  });

  it('should fail to execute due to wrong round', async function () {
    await context.polkadotApi.tx.bfcStaking
      .executeNominationRequest(alith.address)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeNominationRequest');
    expect(extrinsicResult).equal('PendingNominationRequestNotDueYet');
  });

  it('should successfully execute scheduled revoke nomination', async function () {
    this.timeout(20000);

    const balanceBefore = new BigNumber((await context.web3.eth.getBalance(charleth.address)).toString());

    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap();
    const nominatorRequestsBefore = nominatorStateBefore.requests.toJSON();

    let validator = null;
    Object.keys(nominatorRequestsBefore['requests']).forEach(function (key) {
      validator = key.toLowerCase();
    });

    expect(validator).equal(alith.address.toLowerCase());

    let whenExecutable = null;
    Object.values(nominatorRequestsBefore['requests']).forEach(function (value: any) {
      whenExecutable = value.whenExecutable;
    });
    expect(whenExecutable).to.be.not.null;

    await jumpToRound(context, Number(whenExecutable));

    await context.polkadotApi.tx.bfcStaking
      .executeNominationRequest(alith.address)
      .signAndSend(charleth);

    await context.createBlock();
    await context.createBlock();

    const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorStateAfter = rawNominatorStateAfter.toHuman();
    expect(nominatorStateAfter).to.be.null;

    const balanceAfter = new BigNumber((await context.web3.eth.getBalance(charleth.address)).toString());
    expect(balanceAfter.isGreaterThan(balanceBefore)).equal(true);
  });
});

describeDevNode('pallet_bfc_staking - leave nominators', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  before('should successfully nominate to alith', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .nominate(alith.address, stake.toFixed(), 0, 0)
      .signAndSend(charleth);

    await context.createBlock();
  });

  it('should fail due to empty nominations', async function () {
    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveNominators()
      .signAndSend(baltathar);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleLeaveNominators');
    expect(extrinsicResult).equal('NominatorDNE');
  });

  it('should successfully schedule leave nominators', async function () {
    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveNominators()
      .signAndSend(charleth);

    await context.createBlock();

    const rawRoundDelay: any = context.polkadotApi.consts.bfcStaking.leaveNominatorsDelay;
    const roundDelay = rawRoundDelay.toNumber();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    const rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorState = rawNominatorState.unwrap();

    expect(nominatorState.status.isLeaving).equal(true);
    expect(nominatorState.status.asLeaving.toNumber()).equal(currentRound + roundDelay);
  });

  it('should fail due to duplicate requests', async function () {
    await context.polkadotApi.tx.bfcStaking
      .scheduleLeaveNominators()
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleLeaveNominators');
    expect(extrinsicResult).equal('NominatorAlreadyLeaving');
  });

  // it only cares if the nominationCount is below to the actual amount
  // if the requested amount is over the actual amount it will pass
  it('should fail to execute scheduled leave nominators due to invalid nominationCount', async function () {
    await context.polkadotApi.tx.bfcStaking
      .executeLeaveNominators(0)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeLeaveNominators');
    expect(extrinsicResult).equal('TooLowNominationCountToLeaveNominators');
  });

  it('should fail execute due to wrong round', async function () {
    await context.polkadotApi.tx.bfcStaking
      .executeLeaveNominators(1)
      .signAndSend(charleth);

    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeLeaveNominators');
    expect(extrinsicResult).equal('NominatorCannotLeaveYet');
  });

  it('should successfully execute scheduled leave nominators', async function () {
    this.timeout(20000);

    const balanceBefore = new BigNumber((await context.web3.eth.getBalance(charleth.address)).toString());

    const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorStateBefore = rawNominatorStateBefore.unwrap();

    await jumpToRound(context, nominatorStateBefore.status.asLeaving.toNumber());

    await context.polkadotApi.tx.bfcStaking
      .executeLeaveNominators(1)
      .signAndSend(charleth);

    await context.createBlock();
    await context.createBlock();

    const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
    const nominatorStateAfter = rawNominatorStateAfter.toHuman();
    expect(nominatorStateAfter).to.be.null;

    const balanceAfter = new BigNumber((await context.web3.eth.getBalance(charleth.address)).toString());
    expect(balanceAfter.isGreaterThan(balanceBefore)).equal(true);
  });
});
