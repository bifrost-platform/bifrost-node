import axios from 'axios';
import BigNumber from 'bignumber.js';
import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import {
  BASIC_MAXIMUM_OFFENCE_COUNT, FULL_MAXIMUM_OFFENCE_COUNT
} from '../../constants/config';
import {
  MIN_BASIC_VALIDATOR_STAKING_AMOUNT, MIN_FULL_VALIDATOR_STAKING_AMOUNT
} from '../../constants/currency';
import {
  TEST_CONTROLLERS, TEST_RELAYERS, TEST_STASHES
} from '../../constants/keys';
import { isEventTriggered } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';
import { jumpToRound, jumpToSession } from '../utils';

describeDevNode('pallet_bfc_offences - simple validator offences', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);

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

    const hasAuraKey = await axios.post(
      'http://localhost:9934',
      {
        'jsonrpc': '2.0',
        'method': 'author_hasKey',
        'params': [auraSessionKey, 'aura'],
        'id': 1,
      },
    );
    expect(hasAuraKey.data.result).equal(true);

    const hasGranKey = await axios.post(
      'http://localhost:9934',
      {
        'jsonrpc': '2.0',
        'method': 'author_hasKey',
        'params': [granSessionKey, 'gran'],
        'id': 1,
      },
    );
    expect(hasGranKey.data.result).equal(true);

    const hasImonKey = await axios.post(
      'http://localhost:9934',
      {
        'jsonrpc': '2.0',
        'method': 'author_hasKey',
        'params': [imonlineSessionKey, 'imon'],
        'id': 1,
      },
    );
    expect(hasImonKey.data.result).equal(true);
  });

  before('should successfully disable auto-compounding', async function () {
    await context.polkadotApi.tx.bfcStaking
      .setCandidateRewardDst('Account')
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();
  });

  it('should successfully send heartbeats and trigger session update - alith', async function () {
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
    expect(validators).not.includes(baltathar.address);
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

  it('should fail to send heartbeats due to offence - baltathar', async function () {
    this.timeout(20000);

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

    const blockHash: any = await jumpToSession(context, currentSession + 1);
    const success = await isEventTriggered(
      context,
      blockHash,
      [
        { method: 'SomeOffline', section: 'imOnline' },
        { method: 'Offence', section: 'offences' },
      ],
    );
    expect(success).equal(true);

    const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
    const validatorOffences = rawValidatorOffences.toJSON();
    expect(validatorOffences.latestOffenceRoundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
    expect(validatorOffences.latestOffenceSessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
    expect(validatorOffences.offenceCount).equal(1);
    expect(validatorOffences.aggregatedSlashFraction.toString()).equal(new BigNumber(0.5).multipliedBy(10 ** 7).toFixed());
    expect(validatorOffences.offences.length).equal(1);
    expect(validatorOffences.offences[0].roundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
    expect(validatorOffences.offences[0].sessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
    expect(validatorOffences.offences[0].totalSlash).equal(context.web3.utils.padLeft(
      context.web3.utils.toHex(new BigNumber(5).multipliedBy(10 ** 18).toFixed()),
      32,
    ));
    expect(validatorOffences.offences[0].offenderSlash).equal(context.web3.utils.padLeft(
      context.web3.utils.toHex(new BigNumber(5).multipliedBy(10 ** 18).toFixed()),
      32,
    ));
    expect(validatorOffences.offences[0].nominatorsSlash).equal(0);
    expect(validatorOffences.offences[0].slashFraction.toString()).equal(new BigNumber(0.5).multipliedBy(10 ** 7).toFixed());
  });

  it('should be slashed due to multiple offences - baltathar', async function () {
    this.timeout(20000);

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    const selfBondBeforeSlash = candidateState.bond;

    const rawTreauryId: any = await context.polkadotApi.query.treasury.treasuryId();
    const treasuryId = rawTreauryId.toJSON();
    const potBalanceBefore = new BigNumber(await context.web3.eth.getBalance(treasuryId));

    let offenceLength = 1;
    for (let i = 1; i <= FULL_MAXIMUM_OFFENCE_COUNT; i++) {
      const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
      const currentSession = rawCurrentRound.currentSessionIndex.toNumber();
      const blockHash: any = await jumpToSession(context, currentSession + 1);

      if (i === FULL_MAXIMUM_OFFENCE_COUNT) {
        const selfBondAfterSlash = new BigNumber(selfBondBeforeSlash).minus(new BigNumber(25).multipliedBy(10 ** 18));

        const success = await isEventTriggered(
          context,
          blockHash,
          [
            { method: 'SomeOffline', section: 'imOnline' },
            { method: 'Offence', section: 'offences' },
            { method: 'Slashed', section: 'bfcOffences' },
            { method: 'Slashed', section: 'balances' },
            { method: 'KickedOut', section: 'bfcStaking' },
          ],
        );
        expect(success).equal(true);

        // check if validatorOffences are removed
        const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
        const validatorOffences = rawValidatorOffences.toJSON();
        expect(validatorOffences).to.be.null;

        // check candidate pool
        const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
        const candidatePool = rawCandidatePool.toJSON();
        let isCandidateFound = false;
        for (const candidate of candidatePool) {
          if (candidate.owner === baltathar.address) {
            isCandidateFound = true;
            break;
          }
        }
        expect(isCandidateFound).equal(false);

        // check selected candidates
        const rawSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
        const selectedCandidates = rawSelectedCandidates.toJSON();
        expect(selectedCandidates).not.includes(baltathar.address);

        // check selected full candidates
        const rawSelectedFullCandidates: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
        const selectedFullCandidates = rawSelectedFullCandidates.toJSON();
        expect(selectedFullCandidates).not.includes(baltathar.address);

        // check cached selected candidates
        const rawCurrentRoundV2: any = await context.polkadotApi.query.bfcStaking.round();
        const currentRound = rawCurrentRoundV2.currentRoundIndex.toNumber();
        const rawCachedSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.cachedSelectedCandidates();
        const cachedSelectedCandidates = rawCachedSelectedCandidates.toJSON();
        let isCachedCandidate = false;
        for (const cache of cachedSelectedCandidates) {
          if (cache[0] === currentRound) {
            for (const candidate of cache[1]) {
              if (candidate === baltathar.address) {
                isCachedCandidate = true;
                break;
              }
            }
          }
        }
        expect(isCachedCandidate).equal(false);

        // check candidate info
        const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
        const candidateState = rawCandidateState.unwrap().toJSON();
        expect(candidateState.status).has.key('kickedOut');
        expect(new BigNumber(candidateState.bond).eq(new BigNumber(selfBondAfterSlash))).equal(true);
        expect(new BigNumber(candidateState.votingPower).eq(new BigNumber(selfBondAfterSlash))).equal(true);

        // check treasury pot
        const rawTreauryId: any = await context.polkadotApi.query.treasury.treasuryId();
        const treasuryId = rawTreauryId.toJSON();
        const potBalance = new BigNumber(await context.web3.eth.getBalance(treasuryId));
        expect(potBalance.toFixed()).equal(new BigNumber(25).multipliedBy(10 ** 18).plus(potBalanceBefore).toFixed());

        // check stash account's reserved balance
        const rawBalance: any = await context.polkadotApi.query.system.account(baltatharStash.address);
        const balance = rawBalance.toJSON();
        expect(balance.data.reserved).equal(candidateState.bond);

        // check session validators
        const rawValidators: any = await context.polkadotApi.query.session.validators();
        const validators = rawValidators.toJSON();
        expect(validators).includes(alith.address);
        expect(validators).includes(baltathar.address);

        // check selected relayers
        const rawSelectedRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
        const selectedRelayers = rawSelectedRelayers.toJSON();
        expect(selectedRelayers).not.includes(baltatharRelayer.address);

        // check relayer pool - should exist
        const rawRelayers: any = await context.polkadotApi.query.relayManager.relayerPool();
        const relayers = rawRelayers.toJSON();
        let isRelayerFound = false;
        for (const relayer of relayers) {
          if (relayer.relayer === baltatharRelayer.address) {
            isRelayerFound = true;
          }
        }
        expect(isRelayerFound).equal(true);

        // check bonded controller - should exist
        const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(baltathar.address);
        const bondedController = rawBondedController.unwrap().toJSON();
        expect(bondedController).equal(baltatharRelayer.address);
      } else {
        const success = await isEventTriggered(
          context,
          blockHash,
          [
            { method: 'SomeOffline', section: 'imOnline' },
            { method: 'Offence', section: 'offences' },
          ],
        );
        expect(success).equal(true);

        offenceLength += 1;
        const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
        const validatorOffences = rawValidatorOffences.unwrap().toJSON();
        expect(validatorOffences.latestOffenceRoundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
        expect(validatorOffences.latestOffenceSessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
        expect(validatorOffences.offenceCount).equal(offenceLength);
        expect(validatorOffences.offences.length).equal(offenceLength);
        expect(validatorOffences.offences[i].roundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
        expect(validatorOffences.offences[i].sessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
        expect(validatorOffences.offences[i].totalSlash).equal(context.web3.utils.padLeft(
          context.web3.utils.toHex(new BigNumber(5).multipliedBy(10 ** 18).toFixed()),
          32,
        ));
        expect(validatorOffences.offences[i].offenderSlash).equal(context.web3.utils.padLeft(
          context.web3.utils.toHex(new BigNumber(5).multipliedBy(10 ** 18).toFixed()),
          32,
        ));
        expect(validatorOffences.offences[i].nominatorsSlash).equal(0);
        expect(validatorOffences.offences[i].slashFraction.toString()).equal(new BigNumber(0.5).multipliedBy(10 ** 7).toFixed());
      }
    }
  });

  it('should not add offences to already kicked out validators', async function () {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();

    const blockHash: any = await jumpToSession(context, currentSession + 1);
    const success = await isEventTriggered(
      context,
      blockHash,
      [
        { method: 'SomeOffline', section: 'imOnline' },
        { method: 'Offence', section: 'offences' },
      ],
    );
    expect(success).equal(true);

    const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
    const validatorOffences = rawValidatorOffences.toJSON();
    expect(validatorOffences).to.be.null;
  });

  it('should successfully return back to online state', async function () {
    await context.polkadotApi.tx.bfcStaking
      .goOnline()
      .signAndSend(baltathar);
    await context.createBlock();

    // check candidate info
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.status).has.key('active');

    // check candidate pool
    const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatePool = rawCandidatePool.toJSON();
    let isCandidateFound = false;
    for (const candidate of candidatePool) {
      if (candidate.owner === baltathar.address) {
        isCandidateFound = true;
        break;
      }
    }
    expect(isCandidateFound).equal(true);
  });

  it('should successfully go offline', async function () {
    await context.polkadotApi.tx.bfcStaking
      .goOffline()
      .signAndSend(alith);
    await context.createBlock();

    // check candidate info
    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState.status).has.key('idle');

    // check candidate pool
    const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
    const candidatePool = rawCandidatePool.toJSON();
    let isCandidateFound = false;
    for (const candidate of candidatePool) {
      if (candidate.owner === alith.address) {
        isCandidateFound = true;
        break;
      }
    }
    expect(isCandidateFound).equal(false);

    const rawSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const selectedCandidates = rawSelectedCandidates.toJSON();
    expect(selectedCandidates).not.includes(alith.address);
  });
}, true);

describeDevNode('pallet_bfc_offences - update bond less pending request #1', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);

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

  // cancel pending bond less requests if it may result to insufficient self bonds (lower than minimum bond)
  // if current_bond - request_amount < minimum_bond
  it('should cancel pending requests due to insufficient amount', async function () {
    const stake = new BigNumber(50).multipliedBy(10 ** 18);

    await context.polkadotApi.tx.bfcStaking
      .scheduleCandidateBondLess(stake.toFixed())
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    this.timeout(20000);

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    const selfBondBeforeSlash = candidateState.bond;
    expect(candidateState.request).is.not.null;

    let offenceLength = 0;
    for (let i = 0; i <= FULL_MAXIMUM_OFFENCE_COUNT; i++) {
      const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
      const currentSession = rawCurrentRound.currentSessionIndex.toNumber();
      const blockHash: any = await jumpToSession(context, currentSession + 1);

      if (i === FULL_MAXIMUM_OFFENCE_COUNT) {
        const selfBondAfterSlash = new BigNumber(selfBondBeforeSlash).minus(new BigNumber(25).multipliedBy(10 ** 18));

        const success = await isEventTriggered(
          context,
          blockHash,
          [
            { method: 'SomeOffline', section: 'imOnline' },
            { method: 'Offence', section: 'offences' },
            { method: 'Slashed', section: 'bfcOffences' },
            { method: 'Slashed', section: 'balances' },
            { method: 'KickedOut', section: 'bfcStaking' },
          ],
        );
        expect(success).equal(true);

        const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
        const validatorOffences = rawValidatorOffences.toJSON();
        expect(validatorOffences).to.be.null;

        // check candidate pool
        const rawCandidatePool: any = await context.polkadotApi.query.bfcStaking.candidatePool();
        const candidatePool = rawCandidatePool.toJSON();
        let isCandidateFound = false;
        for (const candidate of candidatePool) {
          if (candidate.owner === baltathar.address) {
            isCandidateFound = true;
            break;
          }
        }
        expect(isCandidateFound).equal(false);

        // check selected candidates
        const rawSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
        const selectedCandidates = rawSelectedCandidates.toJSON();
        expect(selectedCandidates).not.includes(baltathar.address);

        // check selected full candidates
        const rawSelectedFullCandidates: any = await context.polkadotApi.query.bfcStaking.selectedFullCandidates();
        const selectedFullCandidates = rawSelectedFullCandidates.toJSON();
        expect(selectedFullCandidates).not.includes(baltathar.address);

        // check cached selected candidates
        const rawCurrentRoundV2: any = await context.polkadotApi.query.bfcStaking.round();
        const currentRound = rawCurrentRoundV2.currentRoundIndex.toNumber();
        const rawCachedSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.cachedSelectedCandidates();
        const cachedSelectedCandidates = rawCachedSelectedCandidates.toJSON();
        let isCachedCandidate = false;
        for (const cache of cachedSelectedCandidates) {
          if (cache[0] === currentRound) {
            for (const candidate of cache[1]) {
              if (candidate === baltathar.address) {
                isCachedCandidate = true;
                break;
              }
            }
          }
        }
        expect(isCachedCandidate).equal(false);

        // check candidate info
        const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
        const candidateState = rawCandidateState.unwrap().toJSON();
        expect(candidateState.status).has.key('kickedOut');
        expect(new BigNumber(candidateState.bond).eq(new BigNumber(selfBondAfterSlash))).equal(true);
        expect(new BigNumber(candidateState.votingPower).eq(new BigNumber(selfBondAfterSlash))).equal(true);
        expect(candidateState.request).is.null;

        // check stash account's reserved balance
        const rawBalance: any = await context.polkadotApi.query.system.account(baltatharStash.address);
        const balance = rawBalance.toJSON();
        expect(balance.data.reserved).equal(candidateState.bond);

        // check session validators
        const rawValidators: any = await context.polkadotApi.query.session.validators();
        const validators = rawValidators.toJSON();
        expect(validators).includes(alith.address);
        expect(validators).includes(baltathar.address);

        // check selected relayers
        const rawSelectedRelayers: any = await context.polkadotApi.query.relayManager.selectedRelayers();
        const selectedRelayers = rawSelectedRelayers.toJSON();
        expect(selectedRelayers).not.includes(baltatharRelayer.address);

        // check relayer pool - should exist
        const rawRelayers: any = await context.polkadotApi.query.relayManager.relayerPool();
        const relayers = rawRelayers.toJSON();
        let isRelayerFound = false;
        for (const relayer of relayers) {
          if (relayer.relayer === baltatharRelayer.address) {
            isRelayerFound = true;
          }
        }
        expect(isRelayerFound).equal(true);

        // check bonded controller - should exist
        const rawBondedController: any = await context.polkadotApi.query.relayManager.bondedController(baltathar.address);
        const bondedController = rawBondedController.unwrap().toJSON();
        expect(bondedController).equal(baltatharRelayer.address);
      } else {
        const success = await isEventTriggered(
          context,
          blockHash,
          [
            { method: 'SomeOffline', section: 'imOnline' },
            { method: 'Offence', section: 'offences' },
          ],
        );
        expect(success).equal(true);

        offenceLength += 1;
        const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
        const validatorOffences = rawValidatorOffences.unwrap().toJSON();
        expect(validatorOffences.latestOffenceRoundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
        expect(validatorOffences.latestOffenceSessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
        expect(validatorOffences.offenceCount).equal(offenceLength);
        expect(validatorOffences.offences.length).equal(offenceLength);
        expect(validatorOffences.offences[i].roundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
        expect(validatorOffences.offences[i].sessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
        expect(validatorOffences.offences[i].totalSlash).equal(context.web3.utils.padLeft(
          context.web3.utils.toHex(new BigNumber(5).multipliedBy(10 ** 18).toFixed()),
          32,
        ));
        expect(validatorOffences.offences[i].offenderSlash).equal(context.web3.utils.padLeft(
          context.web3.utils.toHex(new BigNumber(5).multipliedBy(10 ** 18).toFixed()),
          32,
        ));
        expect(validatorOffences.offences[i].nominatorsSlash).equal(0);
        expect(validatorOffences.offences[i].slashFraction.toString()).equal(new BigNumber(0.5).multipliedBy(10 ** 7).toFixed());
      }
    }
  });
}, true);

describeDevNode('pallet_bfc_offences - update bond less pending request #2', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);

  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const baltatharStash = keyring.addFromUri(TEST_STASHES[1].private);

  let baltatharAura = '';
  let baltatharGran = '';
  let baltatharImOnline = '';

  before('should successfully join candidate pool and set session keys', async function () {
    const stake = new BigNumber(MIN_BASIC_VALIDATOR_STAKING_AMOUNT);

    await context.polkadotApi.tx.bfcStaking
      .joinCandidates(baltathar.address, null, stake.toFixed(), 1)
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

  // safe to go if bonding requirements are satisfied
  // if self_bond - request_amount >= minimum_bond
  it('should cancel pending requests due to insufficient amount', async function () {
    this.timeout(20000);

    const stake = new BigNumber(100).multipliedBy(10 ** 18);

    await context.polkadotApi.tx.bfcStaking
      .scheduleCandidateBondLess(stake.toFixed())
      .signAndSend(baltathar, { nonce: -1 });
    await context.createBlock();

    const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
    const candidateState = rawCandidateState.unwrap().toJSON();
    const selfBondBeforeSlash = candidateState.bond;
    expect(candidateState.request).is.not.null;

    let offenceLength = 0;
    for (let i = 0; i <= BASIC_MAXIMUM_OFFENCE_COUNT; i++) {
      const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
      const currentSession = rawCurrentRound.currentSessionIndex.toNumber();
      const blockHash: any = await jumpToSession(context, currentSession + 1);

      if (i === BASIC_MAXIMUM_OFFENCE_COUNT) {
        const selfBondAfterSlash = new BigNumber(selfBondBeforeSlash).minus(new BigNumber(7.5).multipliedBy(10 ** 18));

        const success = await isEventTriggered(
          context,
          blockHash,
          [
            { method: 'SomeOffline', section: 'imOnline' },
            { method: 'Offence', section: 'offences' },
            { method: 'Slashed', section: 'bfcOffences' },
            { method: 'Slashed', section: 'balances' },
            { method: 'KickedOut', section: 'bfcStaking' },
          ],
        );
        expect(success).equal(true);

        const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
        const validatorOffences = rawValidatorOffences.toJSON();
        expect(validatorOffences).to.be.null;

        // check candidate info
        const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
        const candidateState = rawCandidateState.unwrap().toJSON();
        expect(candidateState.status).has.key('kickedOut');
        expect(new BigNumber(candidateState.bond).eq(new BigNumber(selfBondAfterSlash))).equal(true);
        expect(new BigNumber(candidateState.votingPower).eq(new BigNumber(selfBondAfterSlash))).equal(true);
        expect(candidateState.request).is.not.null;
      } else {
        const success = await isEventTriggered(
          context,
          blockHash,
          [
            { method: 'SomeOffline', section: 'imOnline' },
            { method: 'Offence', section: 'offences' },
          ],
        );
        expect(success).equal(true);

        offenceLength += 1;
        const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
        const validatorOffences = rawValidatorOffences.unwrap().toJSON();
        expect(validatorOffences.latestOffenceRoundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
        expect(validatorOffences.latestOffenceSessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
        expect(validatorOffences.offenceCount).equal(offenceLength);
        expect(validatorOffences.offences.length).equal(offenceLength);
        expect(validatorOffences.offences[i].roundIndex).equal(rawCurrentRound.currentRoundIndex.toNumber());
        expect(validatorOffences.offences[i].sessionIndex).equal(rawCurrentRound.currentSessionIndex.toNumber());
        expect(validatorOffences.offences[i].totalSlash).equal(context.web3.utils.padLeft(
          context.web3.utils.toHex(new BigNumber(2.5).multipliedBy(10 ** 18).toFixed()),
          32,
        ));
        expect(validatorOffences.offences[i].offenderSlash).equal(context.web3.utils.padLeft(
          context.web3.utils.toHex(new BigNumber(2.5).multipliedBy(10 ** 18).toFixed()),
          32,
        ));
        expect(validatorOffences.offences[i].nominatorsSlash).equal(0);
        expect(validatorOffences.offences[i].slashFraction.toString()).equal(new BigNumber(0.5).multipliedBy(10 ** 7).toFixed());
      }
    }
  });
}, true);

describeDevNode('pallet_bfc_offences - minimum bond requirement', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });

  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);

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

  it('should not select validator due to unsatisfied minimum self bond', async function () {
    this.timeout(20000);

    for (let i = 0; i <= FULL_MAXIMUM_OFFENCE_COUNT; i++) {
      const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
      const currentSession = rawCurrentRound.currentSessionIndex.toNumber();
      const blockHash: any = await jumpToSession(context, currentSession + 1);

      const success = await isEventTriggered(
        context,
        blockHash,
        [
          { method: 'SomeOffline', section: 'imOnline' },
          { method: 'Offence', section: 'offences' },
          { method: 'Slashed', section: 'bfcOffences' },
          { method: 'Slashed', section: 'balances' },
          { method: 'KickedOut', section: 'bfcStaking' },
        ],
      );
      if (success) {
        const rawValidatorOffences: any = await context.polkadotApi.query.bfcOffences.validatorOffences(baltathar.address);
        const validatorOffences = rawValidatorOffences.toJSON();
        expect(validatorOffences).to.be.null;

        // check candidate info
        const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(baltathar.address);
        const candidateState = rawCandidateState.unwrap().toJSON();
        expect(candidateState.status).has.key('kickedOut');
        expect(candidateState.isSelected).equal(false);
      }
    }

    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    await jumpToRound(context, currentRound + 1);

    const rawSelectedCandidates: any = await context.polkadotApi.query.bfcStaking.selectedCandidates();
    const selectedCandidates = rawSelectedCandidates.toJSON();
    expect(selectedCandidates).not.includes(baltathar.address);
  });
}, true);
