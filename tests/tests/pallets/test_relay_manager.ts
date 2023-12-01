import axios from 'axios';
import BigNumber from 'bignumber.js';
import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import {
  MIN_FULL_CANDIDATE_STAKING_AMOUNT, MIN_FULL_VALIDATOR_STAKING_AMOUNT
} from '../../constants/currency';
import {
  TEST_CONTROLLERS, TEST_RELAYERS, TEST_STASHES
} from '../../constants/keys';
import { getExtrinsicResult, isEventTriggered } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';
import { jumpToRound, jumpToSession } from '../utils';

describeDevNode('pallet_relay_manager - set relayer', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);
  const newRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  before('should successfully map the bonded relayer', async function () {
    // check `BondedController`
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(alith.address);
    const bondedController = rawBondedController.toJSON();
    expect(bondedController).equal(alithRelayer.address);

    // check `RelayerPool`
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    let isRelayerFound = false;
    for (const relayer of relayerPool) {
      if (relayer.controller === alith.address && relayer.relayer === alithRelayer.address) {
        isRelayerFound = true;
        break;
      }
    }
    expect(isRelayerFound).equal(true);
    expect(relayerPool.length).equal(1);

    // check `RelayerState`
    const rawRelayerState: any = await context.polkadotApi.query.relayManager.relayerState(alithRelayer.address);
    const relayerState = rawRelayerState.unwrap().toJSON();
    expect(relayerState.controller).equal(alith.address);

    // check `SelectedRelayers`
    const rawRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayers = rawRelayers.toJSON();
    expect(relayers.length).equal(1);
    expect(relayers[0]).equal(alithRelayer.address);

    // check `InitialSelectedRelayers`
    const rawInitialRelayers: any = await context.polkadotApi.query.relayManager.initialSelectedRelayers();
    const initialRelayers = rawInitialRelayers.toJSON();
    expect(initialRelayers.length).equal(1);
    expect(initialRelayers[0]).equal(alithRelayer.address);

    // check `CachedSelectedRelayers`
    const rawCachedRelayers: any = await context.polkadotApi.query.relayManager.cachedSelectedRelayers();
    const cachedRelayers = rawCachedRelayers.toJSON();
    expect(cachedRelayers[0][1].length).equal(1);
    expect(cachedRelayers[0][1]).include(alithRelayer.address);

    // check `CachedInitialSelectedRelayers`
    const rawCachedInitialRelayers: any = await context.polkadotApi.query.relayManager.cachedInitialSelectedRelayers();
    const cachedInitialRelayers = rawCachedInitialRelayers.toJSON();
    expect(cachedInitialRelayers[0][1].length).equal(1);
    expect(cachedInitialRelayers[0][1]).include(alithRelayer.address);
  });

  before('should successfully send a heartbeat', async function () {
    await context.polkadotApi.tx.relayManager.heartbeat()
      .signAndSend(alithRelayer);
    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

    const rawHeartbeat: any = await context.polkadotApi.query.relayManager.receivedHeartbeats(currentSession, alithRelayer.address);
    const heartbeat = rawHeartbeat.toJSON();
    expect(heartbeat).equal(true);

    const rawRelayerState: any = await context.polkadotApi.query.relayManager.relayerState(alithRelayer.address);
    const relayerState = rawRelayerState.unwrap().toJSON();
    expect(relayerState.status).equal('Active');
  });

  it('should successfully replace relayer account', async function () {
    await context.polkadotApi.tx.relayManager
      .setRelayer(newRelayer.address)
      .signAndSend(alith);
    await context.createBlock();

    // check `BondedController`
    const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(alith.address);
    const bondedController = rawBondedController.toJSON();
    expect(bondedController).equal(newRelayer.address);

    // check `RelayerPool`
    const rawRelayerPool: any = await context.polkadotApi.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    let isRelayerFound = false;
    for (const relayer of relayerPool) {
      if (relayer.controller === alith.address && relayer.relayer === newRelayer.address) {
        isRelayerFound = true;
        break;
      }
    }
    expect(isRelayerFound).equal(true);
    expect(relayerPool.length).equal(1);

    // check `RelayerState`
    const rawRelayerState: any = await context.polkadotApi.query.relayManager.relayerState(newRelayer.address);
    const relayerState = rawRelayerState.unwrap().toJSON();
    expect(relayerState.controller).equal(alith.address);

    // check `SelectedRelayers`
    const rawRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
    const relayers = rawRelayers.toJSON();
    expect(relayers.length).equal(1);
    expect(relayers[0]).equal(newRelayer.address);

    // check `InitialSelectedRelayers`
    const rawInitialRelayers: any = await context.polkadotApi.query.relayManager.initialSelectedRelayers();
    const initialRelayers = rawInitialRelayers.toJSON();
    expect(initialRelayers.length).equal(1);
    expect(initialRelayers[0]).equal(alithRelayer.address);

    // check `CachedSelectedRelayers`
    const rawCachedRelayers: any = await context.polkadotApi.query.relayManager.cachedSelectedRelayers();
    const cachedRelayers = rawCachedRelayers.toJSON();
    expect(cachedRelayers[0][1].length).equal(1);
    expect(cachedRelayers[0][1]).include(newRelayer.address);

    // check `CachedInitialSelectedRelayers`
    const rawCachedInitialRelayers: any = await context.polkadotApi.query.relayManager.cachedInitialSelectedRelayers();
    const cachedInitialRelayers = rawCachedInitialRelayers.toJSON();
    expect(cachedInitialRelayers[0][1].length).equal(1);
    expect(cachedInitialRelayers[0][1]).include(alithRelayer.address);

    // check `ReceivedHeartbeats`
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();
    const rawHeartbeat: any = await context.polkadotApi.query.relayManager.receivedHeartbeats(currentSession, newRelayer.address);
    const heartbeat = rawHeartbeat.toJSON();
    expect(heartbeat).equal(true);
  });
});

describeDevNode('pallet_relay_manager - relayer heartbeat', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  const baltatharRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const charlethStash = keyring.addFromUri(TEST_STASHES[2].private);
  const charlethRelayer = keyring.addFromUri(TEST_RELAYERS[2].private);

  it('should successfully activate heartbeat offences', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.relayManager.setHeartbeatOffenceActivation(true),
    ).signAndSend(alith);
    await context.createBlock();

    const isActive = await context.polkadotApi.query.relayManager.isHeartbeatOffenceActive();
    expect(isActive.toJSON()).equal(true);
  });

  it('should fail to send heartbeat due to dne relayer', async function () {
    await context.polkadotApi.tx.relayManager.heartbeat()
      .signAndSend(baltatharRelayer);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'relayManager', 'heartbeat');
    expect(extrinsicResult).equal('RelayerDNE');
  });

  it('should fail to send heartbeat due to inactive relayer', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);
    const candidateAmount = 1;

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(charleth.address, charlethRelayer.address, stake.toFixed(), candidateAmount)
      .signAndSend(charlethStash, { nonce: -1 });
    await context.createBlock();

    const rawRelayerState: any = await context.polkadotApi.query.relayManager.relayerState(charlethRelayer.address);
    const relayerState = rawRelayerState.unwrap().toJSON();
    expect(relayerState.controller).equal(charleth.address);
    expect(relayerState.status).equal('Idle');

    await context.polkadotApi.tx.relayManager.heartbeat()
      .signAndSend(charlethRelayer);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'relayManager', 'heartbeat');
    expect(extrinsicResult).equal('RelayerInactive');
  });

  it('should increase offence due to missed heartbeat', async function () {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

    const blockHash: any = await jumpToSession(context, currentSession + 1);
    const success = await isEventTriggered(
      context,
      blockHash,
      [
        { method: 'SomeOffline', section: 'relayManager' },
        { method: 'Offence', section: 'offences' },
      ],
    );
    expect(success).equal(true);

    const rawRelayerState: any = await context.polkadotApi.query.relayManager.relayerState(alithRelayer.address);
    const relayerState = rawRelayerState.unwrap().toJSON();
    expect(relayerState.status).equal('Idle');

    const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(alith.address);
    const validatorOffences = rawValidatorOffences.unwrap().toJSON();
    expect(validatorOffences.latestOffenceRoundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
    expect(validatorOffences.latestOffenceSessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
    expect(validatorOffences.offenceCount).equal(1);
    expect(validatorOffences.aggregatedSlashFraction.toString()).equal(new BigNumber(1).multipliedBy(10 ** 7).toFixed());
    expect(validatorOffences.offences.length).equal(1);
    expect(validatorOffences.offences[0].roundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
    expect(validatorOffences.offences[0].sessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
    expect(context.web3.utils.hexToNumberString(validatorOffences.offences[0].totalSlash)).equal(new BigNumber(10).multipliedBy(10 ** 18).toFixed());
    expect(context.web3.utils.hexToNumberString(validatorOffences.offences[0].offenderSlash)).equal(new BigNumber(10).multipliedBy(10 ** 18).toFixed());
    expect(validatorOffences.offences[0].nominatorsSlash).equal(0);
    expect(validatorOffences.offences[0].slashFraction.toString()).equal(new BigNumber(1).multipliedBy(10 ** 7).toFixed());
  });

  it('should successfully send heartbeat', async function () {
    await context.polkadotApi.tx.relayManager.heartbeat()
      .signAndSend(alithRelayer);
    await context.createBlock();

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

    const rawHeartbeat: any = await context.polkadotApi.query.relayManager.receivedHeartbeats(currentSession, alithRelayer.address);
    const heartbeat = rawHeartbeat.toJSON();
    expect(heartbeat).equal(true);

    const rawRelayerState: any = await context.polkadotApi.query.relayManager.relayerState(alithRelayer.address);
    const relayerState = rawRelayerState.unwrap().toJSON();
    expect(relayerState.status).equal('Active');
  });

  it('should refresh offences due to active heartbeats', async function () {
    const rawOffenceExpiration: any = await context.polkadotApi.query.bfcOffences.offenceExpirationInSessions();
    for (let idx = 0; idx < rawOffenceExpiration.toNumber(); idx++) {
      const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
      const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

      await context.polkadotApi.tx.relayManager.heartbeat()
        .signAndSend(alithRelayer);
      await context.createBlock();

      const rawHeartbeat: any = await context.polkadotApi.query.relayManager.receivedHeartbeats(currentSession, alithRelayer.address);
      const heartbeat = rawHeartbeat.toJSON();
      expect(heartbeat).equal(true);

      const blockHash: any = await jumpToSession(context, currentSession + 1);

      const success = await isEventTriggered(
        context,
        blockHash,
        [
          { method: 'AllGood', section: 'relayManager' },
        ],
      );
      expect(success).equal(true);
    }
    // check if validatorOffences are removed
    const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(alith.address);
    const validatorOffences = rawValidatorOffences.toJSON();
    expect(validatorOffences).to.be.null;
  });
});

describeDevNode('pallet_relay_manager - relayer kick out', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);
  const baltatharRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  let baltatharAura = '';
  let baltatharGran = '';
  let baltatharImOnline = '';

  before('should successfully join candidate pool and set session keys', async function () {
    const stake = new BigNumber(MIN_FULL_VALIDATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, baltatharRelayer.address, stake.toFixed(), 1)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();

    const response = await axios.post(
      'http://localhost:9934',
      {
        'jsonrpc': '2.0',
        'method': 'author_rotateKeys',
        'id': 1,
      },
    );
    const sessionKey = response.data.result.slice(2);
    const auraSessionKey = `0x${sessionKey.slice(0, 64)}`;
    const granSessionKey = `0x${sessionKey.slice(64, 128)}`;
    const imonlineSessionKey = `0x${sessionKey.slice(128)}`;

    baltatharAura = auraSessionKey;
    baltatharGran = granSessionKey;
    baltatharImOnline = imonlineSessionKey;

    const keys: any = {
      aura: auraSessionKey,
      grandpa: granSessionKey,
      imOnline: imonlineSessionKey,
    };
    await context.polkadotApi.tx.session.setKeys(keys, '0x00').signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  before('should successfully set auto-compound to account', async function () {
    await context.polkadotApi.tx.bfcStaking
      .setCandidateRewardDst('Account')
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  it('should be added to queued keys on the next round - baltathar', async function () {
    this.timeout(20000);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);

    const rawQueuedKeys: any = await context.polkadotApi.query.session.queuedKeys();
    const queuedKeys = rawQueuedKeys.toJSON();
    let isKeyFound = false;
    for (const key of queuedKeys) {
      if (key[0] === baltathar.address) {
        isKeyFound = true;
        expect(key[1].aura).equal(baltatharAura);
        expect(key[1].grandpa).equal(baltatharGran);
        expect(key[1].imOnline).equal(baltatharImOnline);
        break;
      }
    }
    expect(isKeyFound).equal(true);

    const rawValidators: any = await context.polkadotApi.query.session.validators();
    const validators = rawValidators.toJSON();
    expect(validators).includes(alith.address);
    expect(validators).not.includes(baltathar.address);
  });

  it('should be added to session validators after one session - baltathar', async function () {
    this.timeout(20000);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

    const blockHash: any = await jumpToSession(context, currentSession + 1);
    const success = await isEventTriggered(
      context,
      blockHash,
      [
        { method: 'AllGood', section: 'imOnline' },
        { method: 'NewSession', section: 'session' },
      ],
    );
    expect(success).equal(true);

    const rawValidators: any = await context.polkadotApi.query.session.validators();
    const validators = rawValidators.toJSON();
    expect(validators).includes(alith.address);
    expect(validators).includes(baltathar.address);
  });

  it('should successfully activate heartbeat offences', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.relayManager.setHeartbeatOffenceActivation(true),
    ).signAndSend(alith);
    await context.createBlock();

    const isActive = await context.polkadotApi.query.relayManager.isHeartbeatOffenceActive();
    expect(isActive.toJSON()).equal(true);
  });

  it('should be kicked out due to inactive heartbeats - baltathar', async function () {
    const rawMaximumOffenceCount: any = await context.polkadotApi.query.bfcOffences.fullMaximumOffenceCount();
    for (let idx = 0; idx < rawMaximumOffenceCount.toNumber(); idx++) {
      const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
      const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

      await context.polkadotApi.tx.relayManager.heartbeat()
        .signAndSend(alithRelayer);
      await context.createBlock();

      const rawHeartbeat: any = await context.polkadotApi.query.relayManager.receivedHeartbeats(currentSession, alithRelayer.address);
      const heartbeat = rawHeartbeat.toJSON();
      expect(heartbeat).equal(true);

      const blockHash: any = await jumpToSession(context, currentSession + 1);

      const success = await isEventTriggered(
        context,
        blockHash,
        [
          { method: 'SomeOffline', section: 'relayManager' },
          { method: 'Offence', section: 'offences' },
          { method: 'Slashed', section: 'bfcOffences' },
          { method: 'Slashed', section: 'balances' },
          { method: 'KickedOut', section: 'bfcStaking' },
        ],
      );
      if (success) {
        const rawRelayerState: any = await context.polkadotApi.query.relayManager.relayerState(alithRelayer.address);
        const relayerState = rawRelayerState.unwrap().toJSON();
        expect(relayerState.status).equal('Active');

        const rawRelayerStateB: any = await context.polkadotApi.query.relayManager.relayerState(baltatharRelayer.address);
        const relayerStateB = rawRelayerStateB.unwrap().toJSON();
        expect(relayerStateB.status).equal('KickedOut');

        const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
        const candidateState = rawCandidateState.unwrap().toJSON();
        expect(candidateState.status).has.key('kickedOut');
        break;
      } else {
        const success = await isEventTriggered(
          context,
          blockHash,
          [
            { method: 'SomeOffline', section: 'relayManager' },
            { method: 'Offence', section: 'offences' },
          ],
        );
        expect(success).equal(true);
      }
    }
  });
}, true);

describeDevNode('pallet_relay_manager - leave request cancelled', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);
  const baltatharRelayer = keyring.addFromUri(TEST_RELAYERS[1].private);

  let baltatharAura = '';
  let baltatharGran = '';
  let baltatharImOnline = '';

  before('should successfully join candidate pool and set session keys', async function () {
    const stake = new BigNumber(MIN_FULL_VALIDATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, baltatharRelayer.address, stake.toFixed(), 1)
      .signAndSend(baltatharStash, { nonce: -1 });
    await context.createBlock();

    const response = await axios.post(
      'http://localhost:9934',
      {
        'jsonrpc': '2.0',
        'method': 'author_rotateKeys',
        'id': 1,
      },
    );
    const sessionKey = response.data.result.slice(2);
    const auraSessionKey = `0x${sessionKey.slice(0, 64)}`;
    const granSessionKey = `0x${sessionKey.slice(64, 128)}`;
    const imonlineSessionKey = `0x${sessionKey.slice(128)}`;

    baltatharAura = auraSessionKey;
    baltatharGran = granSessionKey;
    baltatharImOnline = imonlineSessionKey;

    const keys: any = {
      aura: auraSessionKey,
      grandpa: granSessionKey,
      imOnline: imonlineSessionKey,
    };
    await context.polkadotApi.tx.session.setKeys(keys, '0x00').signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  before('should successfully set auto-compound to account', async function () {
    await context.polkadotApi.tx.bfcStaking
      .setCandidateRewardDst('Account')
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  it('should be added to queued keys on the next round - baltathar', async function () {
    this.timeout(20000);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

    await jumpToRound(context, currentRound + 1);

    const rawQueuedKeys: any = await context.polkadotApi.query.session.queuedKeys();
    const queuedKeys = rawQueuedKeys.toJSON();
    let isKeyFound = false;
    for (const key of queuedKeys) {
      if (key[0] === baltathar.address) {
        isKeyFound = true;
        expect(key[1].aura).equal(baltatharAura);
        expect(key[1].grandpa).equal(baltatharGran);
        expect(key[1].imOnline).equal(baltatharImOnline);
        break;
      }
    }
    expect(isKeyFound).equal(true);

    const rawValidators: any = await context.polkadotApi.query.session.validators();
    const validators = rawValidators.toJSON();
    expect(validators).includes(alith.address);
    expect(validators).not.includes(baltathar.address);
  });

  it('should be added to session validators after one session - baltathar', async function () {
    this.timeout(20000);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

    const blockHash: any = await jumpToSession(context, currentSession + 1);
    const success = await isEventTriggered(
      context,
      blockHash,
      [
        { method: 'AllGood', section: 'imOnline' },
        { method: 'NewSession', section: 'session' },
      ],
    );
    expect(success).equal(true);

    const rawValidators: any = await context.polkadotApi.query.session.validators();
    const validators = rawValidators.toJSON();
    expect(validators).includes(alith.address);
    expect(validators).includes(baltathar.address);
  });

  it('should successfully activate heartbeat offences', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.relayManager.setHeartbeatOffenceActivation(true),
    ).signAndSend(alith);
    await context.createBlock();

    const isActive = await context.polkadotApi.query.relayManager.isHeartbeatOffenceActive();
    expect(isActive.toJSON()).equal(true);
  });

  it('should successfully schedule candidate leave request', async function () {
    await context.polkadotApi.tx.bfcStaking.scheduleLeaveCandidates(10)
      .signAndSend(baltathar);
    await context.createBlock();

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.status).has.key('leaving');
  });

  it('should be kicked out and prior leave request cancelled - baltathar', async function () {
    const rawMaximumOffenceCount: any = await context.polkadotApi.query.bfcOffences.fullMaximumOffenceCount();
    for (let idx = 0; idx < rawMaximumOffenceCount.toNumber(); idx++) {
      const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
      const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

      await context.polkadotApi.tx.relayManager.heartbeat()
        .signAndSend(alithRelayer);
      await context.createBlock();

      const rawHeartbeat: any = await context.polkadotApi.query.relayManager.receivedHeartbeats(currentSession, alithRelayer.address);
      const heartbeat = rawHeartbeat.toJSON();
      expect(heartbeat).equal(true);

      const blockHash: any = await jumpToSession(context, currentSession + 1);

      const success = await isEventTriggered(
        context,
        blockHash,
        [
          { method: 'SomeOffline', section: 'relayManager' },
          { method: 'Offence', section: 'offences' },
          { method: 'Slashed', section: 'bfcOffences' },
          { method: 'Slashed', section: 'balances' },
          { method: 'KickedOut', section: 'bfcStaking' },
        ],
      );
      if (success) {
        const rawRelayerState: any = await context.polkadotApi.query.relayManager.relayerState(alithRelayer.address);
        const relayerState = rawRelayerState.unwrap().toJSON();
        expect(relayerState.status).equal('Active');

        const rawRelayerStateB: any = await context.polkadotApi.query.relayManager.relayerState(baltatharRelayer.address);
        const relayerStateB = rawRelayerStateB.unwrap().toJSON();
        expect(relayerStateB.status).equal('KickedOut');

        const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
        const candidateState = rawCandidateState.unwrap().toJSON();
        expect(candidateState.status).has.key('kickedOut');
      }
    }
  });
}, true);
