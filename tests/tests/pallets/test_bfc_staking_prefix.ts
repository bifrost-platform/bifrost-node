import BigNumber from 'bignumber.js';
import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import {
  AMOUNT_FACTOR, MIN_NOMINATOR_STAKING_AMOUNT,
  MIN_NOMINATOR_TOTAL_STAKING_AMOUNT
} from '../../constants/currency';
import {
  TEST_CONTROLLERS,
} from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';
import { jumpToRound } from '../utils';
// import { number } from 'yargs';

// const DEFAULT_ROUND_LENGTH = 40;

describeDevNode('pallet_bfc_staking - prefix nomination unit testing #ok_to_decrease', (context) => {
    const keyring = new Keyring({ type: 'ethereum' });
    const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
    const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
    const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
    const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);

    const minStake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);
    const minTotalstake =new BigNumber(MIN_NOMINATOR_TOTAL_STAKING_AMOUNT);

    const extraStake =minStake.plus(AMOUNT_FACTOR);
    const extraTotalstake =minTotalstake.plus(AMOUNT_FACTOR);

    before('should successfully nominate to alith', async function () {

        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 0, 0)
            .signAndSend(baltathar);

        await context.createBlock();

        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);


        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 1, 1)
            .signAndSend(charleth);

        await context.createBlock();

        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);

    });

    it('#1', async function () {

        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(dorothy.address, minStake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal('NominatorDNE');

    });

    it('#4', async function () {

        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(alith.address, extraTotalstake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal('NominationBelowMin');

    });

    it('#5', async function () {

        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(alith.address, extraStake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal('NominatorBondBelowMin');

    });

    it('#2', async function () {

        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(alith.address, minStake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal(null);
    
        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(alith.address, minStake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal('PendingNominationRequestAlreadyExists');

    });

    it('#3', async function () {

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
    
        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(alith.address, minStake.toFixed())
            .signAndSend(charleth);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal('NominatorAlreadyLeaving');

    });

});


describeDevNode('pallet_bfc_staking - prefix nomination unit testing #ok_to_revoke', (context) => {
    const keyring = new Keyring({ type: 'ethereum' });
    const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
    const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
    const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
    const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);

    const minStake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);
    const minTotalstake =new BigNumber(MIN_NOMINATOR_TOTAL_STAKING_AMOUNT);

    // const extraStake =minStake.plus(AMOUNT_FACTOR);
    const extraTotalstake =minTotalstake.plus(AMOUNT_FACTOR);

    before('should successfully nominate to alith', async function () {

        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 0, 0)
            .signAndSend(baltathar);

        await context.createBlock();

        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);


        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 1, 1)
            .signAndSend(charleth);

        await context.createBlock();

        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);
    
    });

    it('#1', async function () {

        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(dorothy.address, minStake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal('NominatorDNE');

    });

    // it('#5', async function () {

    //     await context.polkadotApi.tx.bfcStaking
    //         .scheduleRevokeNomination(alith.address)
    //         .signAndSend(baltathar);
    
    //     await context.createBlock();
    
    //     let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleRevokeNomination');
    //     expect(extrinsicResult).equal('NominationDNE');

    // });

    it('#2', async function () {

        await context.polkadotApi.tx.bfcStaking
            .scheduleRevokeNomination(alith.address)
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleRevokeNomination');
        expect(extrinsicResult).equal(null);
    
        await context.polkadotApi.tx.bfcStaking
            .scheduleRevokeNomination(alith.address)
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleRevokeNomination');
        expect(extrinsicResult).equal('PendingNominationRequestAlreadyExists');

    });

    // it('#4', async function () {

    //     const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
    //     const nominatorStateBefore = rawNominatorStateBefore.unwrap();
    //     const nominatorRequestsBefore = nominatorStateBefore.requests.toJSON();

    //     let validator = null;
    //     Object.keys(nominatorRequestsBefore['requests']).forEach(function (key) {
    //     validator = key.toLowerCase();
    //     });

    //     expect(validator).equal(alith.address.toLowerCase());

    //     let whenExecutable = null;
    //     Object.values(nominatorRequestsBefore['requests']).forEach(function (value: any) {
    //     whenExecutable = value.whenExecutable;
    //     });
    //     expect(whenExecutable).to.be.not.null;

    //     await jumpToRound(context, Number(whenExecutable));

    //     await context.polkadotApi.tx.bfcStaking
    //     .executeNominationRequest(alith.address)
    //     .signAndSend(baltathar);

    //     await context.createBlock();
    //     await context.createBlock();

    //     await context.polkadotApi.tx.bfcStaking
    //         .scheduleNominatorBondLess(alith.address, extraStake.toFixed())
    //         .signAndSend(baltathar);
    
    //     await context.createBlock();

    // });

    it('#3', async function () {

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
    
        await context.polkadotApi.tx.bfcStaking
            .scheduleRevokeNomination(alith.address)
            .signAndSend(charleth);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleRevokeNomination');
        expect(extrinsicResult).equal('NominatorAlreadyLeaving');

    });

});

describeDevNode('pallet_bfc_staking - prefix nomination unit testing #schedule_decrease_nomination', (context) => {
    const keyring = new Keyring({ type: 'ethereum' });
    const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
    const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
    const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
    // const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);

    const minStake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);
    const minTotalstake =new BigNumber(MIN_NOMINATOR_TOTAL_STAKING_AMOUNT);

    // const extraStake =minStake.plus(AMOUNT_FACTOR);
    const extraTotalstake =minTotalstake.plus(AMOUNT_FACTOR);

    before('should successfully nominate to alith', async function () {

        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 0, 0)
            .signAndSend(baltathar);

        await context.createBlock();

        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);


        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 1, 1)
            .signAndSend(charleth);

        await context.createBlock();

        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);
    
    });

    it('#1', async function () {
        
        let expectAmount = extraTotalstake.minus(AMOUNT_FACTOR);

        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(alith.address, minStake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal(null);

        const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
        const nominatorStateAfter = rawNominatorStateAfter.unwrap().toJSON();

        const rawTopNominations: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
        const topNominations = rawTopNominations.unwrap();

        const rawCandidateState: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
        const candidateState = rawCandidateState.unwrap();

        const totalStaked: any = await context.polkadotApi.query.bfcStaking.total();

        const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
        const currentRound = rawCurrentRound.currentRoundIndex.toNumber();

        const selfBondBefore = new BigNumber(candidateState.bond.toString());
        const expectedStake = selfBondBefore.plus(extraTotalstake).plus(expectAmount);
        
        // Decrease Nominator`s given candidate`s balance by given amount
        expect(parseInt(nominatorStateAfter.nominations[alith.address].toString(), 16).toString()).equal(expectAmount.toFixed());

        // Decrease Nominator`s total  by given amount
        expect(parseInt(nominatorStateAfter.total.toString(), 16).toString()).equal(expectAmount.toFixed());

        // Decrease top nomination of given candidate`s given Nominator`s amount
        expect(topNominations.nominations[1].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
        expect(topNominations.nominations[1].amount.toString()).equal(expectAmount.toFixed());

        // Decrease given candidate`s voting power
        expect(candidateState.votingPower.toString()).equal(expectedStake.toFixed());

        // Decrease Total amount staked
        expect(totalStaked.toString()).equal(expectedStake.toFixed());

        // Decrease TotalSnapshot total_stake & active_stake & total_voting_power & active_voting_power
        await jumpToRound(context, currentRound + 1);
        const rawTotalSnapshot: any = await context.polkadotApi.query.bfcStaking.totalAtStake(currentRound + 1);
        const totalSnapshot = rawTotalSnapshot.unwrap().toJSON();

        expect(context.web3.utils.hexToNumberString(totalSnapshot.totalStake)).equal(expectedStake.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshot.activeStake)).equal(expectedStake.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshot.totalVotingPower)).equal(expectedStake.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshot.activeVotingPower)).equal(expectedStake.toFixed());

        // Decrease ValidatorSnapshot total amount
        const rawAtStake: any = await context.polkadotApi.query.bfcStaking.atStake(currentRound + 1, alith.address);
        const atStake = rawAtStake.toJSON();
        expect(context.web3.utils.hexToNumberString(atStake.total)).equal(expectedStake.toFixed());

        // Decrease ValidatorSnapshot nominations`s given nominator`s amount
        expect(atStake.nominations[1].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
        expect(context.web3.utils.hexToNumberString(atStake.nominations[1].amount)).equal(expectAmount.toFixed());

    });

});


describeDevNode('pallet_bfc_staking - prefix nomination unit testing #execute_pending_request', (context) => {
    const keyring = new Keyring({ type: 'ethereum' });
    const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
    const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
    const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
    // const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);

    const minStake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);
    const minTotalstake =new BigNumber(MIN_NOMINATOR_TOTAL_STAKING_AMOUNT);

    // const extraStake =minStake.plus(AMOUNT_FACTOR);
    const extraTotalstake =minTotalstake.plus(AMOUNT_FACTOR);

    before('should successfully nominate to alith', async function () {

        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 0, 0)
            .signAndSend(baltathar);

        await context.createBlock();

        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);


        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 1, 1)
            .signAndSend(charleth);

        await context.createBlock();

        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);

    });

    it('#1', async function () {

        await context.polkadotApi.tx.bfcStaking
            .executeNominationRequest(alith.address)
            .signAndSend(baltathar);

        await context.createBlock();

        const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeNominationRequest');
        expect(extrinsicResult).equal('PendingNominationRequestDNE');


    });
    
    it('#2', async function () {

        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(alith.address, minStake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();

        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal(null);

        await context.polkadotApi.tx.bfcStaking
            .executeNominationRequest(alith.address)
            .signAndSend(baltathar);

        await context.createBlock();

        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeNominationRequest');
        expect(extrinsicResult).equal('PendingNominationRequestNotDueYet');

    });

    it('#3', async function () {
        
        let expectAmount = extraTotalstake.minus(AMOUNT_FACTOR);

        const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
        const nominatorStateBefore = rawNominatorStateBefore.unwrap();
        const nominatorRequestsBefore = nominatorStateBefore.requests.toJSON();

        const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
        const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
        await jumpToRound(context, currentRound + 1);

        await context.polkadotApi.tx.bfcStaking
            .executeNominationRequest(alith.address)
            .signAndSend(baltathar);

        await context.createBlock();

        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeNominationRequest');
        expect(extrinsicResult).equal(null);

        const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
        const nominatorStateAfter = rawNominatorStateAfter.unwrap();
        const nominatorRequestsAfter = nominatorStateAfter.requests.toJSON();

        // Decrease less_total
        expect(context.web3.utils.hexToNumberString(nominatorRequestsBefore.lessTotal)).equal(minStake.toFixed());
        expect(nominatorRequestsAfter.lessTotal).equal(0);

        // Unreserve given amount
        const account = await context.polkadotApi.query.system.account(baltathar.address);
        expect(account['data'].reserved.toString()).equal(expectAmount.toFixed());

    });

    it('#4', async function () {

        let expectAmount = extraTotalstake.minus(AMOUNT_FACTOR);

        // baltathar
        let account = await context.polkadotApi.query.system.account(baltathar.address);
        expect(account['data'].reserved.toString()).equal(expectAmount.toFixed());

        let rawNominatorState: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
        let nominatorState = rawNominatorState.unwrap().toJSON();
        expect(nominatorState.nominations).has.key(alith.address);
        expect(parseInt(nominatorState.nominations[alith.address].toString(), 16).toString()).equal(expectAmount.toFixed());

        // charleth
        account = await context.polkadotApi.query.system.account(charleth.address);
        expect(account['data'].reserved.toString()).equal(extraTotalstake.toFixed());

        rawNominatorState = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
        nominatorState = rawNominatorState.unwrap().toJSON();
        expect(nominatorState.nominations).has.key(alith.address);
        expect(parseInt(nominatorState.nominations[alith.address].toString(), 16).toString()).equal(extraTotalstake.toFixed());
    
        await context.polkadotApi.tx.bfcStaking
            .scheduleRevokeNomination(alith.address)
            .signAndSend(charleth);
    
        await context.createBlock();

        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleRevokeNomination');
        expect(extrinsicResult).equal(null);

        const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
        const nominatorStateBefore = rawNominatorStateBefore.unwrap();
        const nominatorRequestsBefore = nominatorStateBefore.requests.toJSON();

        const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
        const candidateStateBefore = rawCandidateStateBefore.unwrap();

        const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
        const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
        await jumpToRound(context, currentRound + 1);

        await context.polkadotApi.tx.bfcStaking
            .executeNominationRequest(alith.address)
            .signAndSend(charleth);

        await context.createBlock();

        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'executeNominationRequest');
        expect(extrinsicResult).equal(null);

        const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(charleth.address);
        const nominatorStateAfter = rawNominatorStateAfter.unwrap();
        const nominatorRequestsAfter = nominatorStateAfter.requests.toJSON();

        const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
        const candidateStateAfter = rawCandidateStateAfter.unwrap();

        // Decrease less_total
        expect(context.web3.utils.hexToNumberString(nominatorRequestsBefore.lessTotal)).equal(extraTotalstake.toFixed());
        expect(nominatorRequestsAfter.lessTotal).equal(0);

        // Decrease revocations count
        expect(nominatorRequestsBefore.revocationsCount).equal(1);
        expect(nominatorRequestsAfter.revocationsCount).equal(0);

        // Decrease nomination count
        expect(candidateStateBefore.nominationCount.toString()).equal('2');
        expect(candidateStateAfter.nominationCount.toString()).equal('1');

        // Remove nominations
        expect(nominatorStateBefore.toJSON().nominations).has.key(alith.address);
        expect(nominatorStateAfter.toJSON().nominations).not.has.key(alith.address);

        // Remove initial nominations
        expect(nominatorStateBefore.toJSON().initialNominations).has.key(alith.address);
        expect(nominatorStateAfter.toJSON().initialNominations).not.has.key(alith.address);

        // Remove awarded tokens per candidate
        expect(nominatorStateBefore.toJSON().awardedTokensPerCandidate).has.key(alith.address);
        expect(nominatorStateAfter.toJSON().awardedTokensPerCandidate).not.has.key(alith.address);

        // Unreserve given amount
        account = await context.polkadotApi.query.system.account(charleth.address);
        expect(account['data'].reserved.toString()).equal('0');

    });

});

describeDevNode('pallet_bfc_staking - prefix nomination unit testing #cancel_pending_request', (context) => {
    const keyring = new Keyring({ type: 'ethereum' });
    const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
    const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
    const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
    // const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);

    const minStake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);
    const minTotalstake =new BigNumber(MIN_NOMINATOR_TOTAL_STAKING_AMOUNT);

    // const extraStake =minStake.plus(AMOUNT_FACTOR);
    const extraTotalstake =minTotalstake.plus(AMOUNT_FACTOR);

    before('should successfully nominate to alith', async function () {

        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 0, 0)
            .signAndSend(baltathar);

        await context.createBlock();

        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);


        await context.polkadotApi.tx.bfcStaking
            .nominate(alith.address, extraTotalstake.toFixed(), 1, 1)
            .signAndSend(charleth);

        await context.createBlock();

        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'nominate');
        expect(extrinsicResult).equal(null);

    });

    it('#1', async function () {

        await context.polkadotApi.tx.bfcStaking
            .cancelNominationRequest(alith.address)
            .signAndSend(baltathar);

        await context.createBlock();

        const extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'cancelNominationRequest');
        expect(extrinsicResult).equal('PendingNominationRequestDNE');

    });

    it('#2', async function () {
        
        let expectAmount = extraTotalstake.minus(AMOUNT_FACTOR);

        // Do bond less
        await context.polkadotApi.tx.bfcStaking
            .scheduleNominatorBondLess(alith.address, minStake.toFixed())
            .signAndSend(baltathar);
    
        await context.createBlock();
    
        let extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'scheduleNominatorBondLess');
        expect(extrinsicResult).equal(null);

        const rawNominatorStateBefore: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
        const nominatorStateBefore = rawNominatorStateBefore.unwrap().toJSON();
        const nominatorRequestsBefore = rawNominatorStateBefore.unwrap().requests.toJSON();

        const rawTopNominationsBefore: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
        const topNominationsBefore = rawTopNominationsBefore.unwrap();

        const rawCandidateStateBefore: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
        const candidateStateBefore = rawCandidateStateBefore.unwrap();

        const totalStakedBefore: any = await context.polkadotApi.query.bfcStaking.total();

        const rawCurrentRoundBefore: any = await context.polkadotApi.query.bfcStaking.round();
        const previousRound = rawCurrentRoundBefore.currentRoundIndex.toNumber();

        await jumpToRound(context, previousRound + 1);
        const rawTotalSnapshotBefore: any = await context.polkadotApi.query.bfcStaking.totalAtStake(previousRound + 1);
        const totalSnapshotBefore = rawTotalSnapshotBefore.unwrap().toJSON();

        const rawAtStakeBefore: any = await context.polkadotApi.query.bfcStaking.atStake(previousRound + 1, alith.address);
        const atStakeBefore = rawAtStakeBefore.toJSON();

        // Do cancel bond less
        await context.polkadotApi.tx.bfcStaking
            .cancelNominationRequest(alith.address)
            .signAndSend(baltathar);

        await context.createBlock();

        extrinsicResult = await getExtrinsicResult(context, 'bfcStaking', 'cancelNominationRequest');
        expect(extrinsicResult).equal(null);

        const rawNominatorStateAfter: any = await context.polkadotApi.query.bfcStaking.nominatorState(baltathar.address);
        const nominatorStateAfter = rawNominatorStateAfter.unwrap().toJSON();
        const nominatorRequestsAfter = rawNominatorStateAfter.unwrap().requests.toJSON();

        const rawTopNominationsAfter: any = await context.polkadotApi.query.bfcStaking.topNominations(alith.address);
        const topNominationsAfter = rawTopNominationsAfter.unwrap();

        const rawCandidateStateAfter: any = await context.polkadotApi.query.bfcStaking.candidateInfo(alith.address);
        const candidateStateAfter = rawCandidateStateAfter.unwrap();

        const totalStakedAfter: any = await context.polkadotApi.query.bfcStaking.total();

        const rawCurrentRoundAfter: any = await context.polkadotApi.query.bfcStaking.round();
        const currentRound = rawCurrentRoundAfter.currentRoundIndex.toNumber();

        await jumpToRound(context, currentRound + 1);
        const rawTotalSnapshotAfter: any = await context.polkadotApi.query.bfcStaking.totalAtStake(previousRound + 1);
        const totalSnapshotAfter = rawTotalSnapshotAfter.unwrap().toJSON();

        const selfBondBefore = new BigNumber(candidateStateBefore.bond.toString());
        const expectedStakeBefore = selfBondBefore.plus(extraTotalstake).plus(expectAmount);

        const selfBondAfter = new BigNumber(candidateStateAfter.bond.toString());
        const expectedStakeAfter = selfBondAfter.plus(extraTotalstake).plus(extraTotalstake);

        const rawAtStakeAfter: any = await context.polkadotApi.query.bfcStaking.atStake(currentRound + 1, alith.address);
        const atStakeAfter = rawAtStakeAfter.toJSON();

        // Decrease less_total
        expect(context.web3.utils.hexToNumberString(nominatorRequestsBefore.lessTotal)).equal(minStake.toFixed());
        expect(nominatorRequestsAfter.lessTotal).equal(0);
        
        // Increase Nominator`s given candidate`s balance by given amount
        expect(parseInt(nominatorStateBefore.nominations[alith.address].toString(), 16).toString()).equal(expectAmount.toFixed());
        expect(parseInt(nominatorStateAfter.nominations[alith.address].toString(), 16).toString()).equal(extraTotalstake.toFixed());

        // Increase Nominator`s total  by given amount
        expect(parseInt(nominatorStateBefore.total.toString(), 16).toString()).equal(expectAmount.toFixed());
        expect(parseInt(nominatorStateAfter.total.toString(), 16).toString()).equal(extraTotalstake.toFixed());

        // Increase top nomination of given candidate`s given Nominator`s amount
        expect(topNominationsBefore.nominations[1].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
        expect(topNominationsBefore.nominations[1].amount.toString()).equal(expectAmount.toFixed());
        expect(topNominationsAfter.nominations[1].amount.toString()).equal(extraTotalstake.toFixed());

        // Increase given candidate`s voting power
        expect(candidateStateBefore.votingPower.toString()).equal(expectedStakeBefore.toFixed());
        expect(candidateStateAfter.votingPower.toString()).equal(expectedStakeAfter.toFixed());

        // Increase Total amount staked
        expect(totalStakedBefore.toString()).equal(expectedStakeBefore.toFixed());
        expect(totalStakedAfter.toString()).equal(expectedStakeAfter.toFixed());

        // Increase TotalSnapshot total_stake & active_stake & total_voting_power & active_voting_power
        expect(context.web3.utils.hexToNumberString(totalSnapshotBefore.totalStake)).equal(expectedStakeBefore.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshotBefore.activeStake)).equal(expectedStakeBefore.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshotBefore.totalVotingPower)).equal(expectedStakeBefore.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshotBefore.activeVotingPower)).equal(expectedStakeBefore.toFixed());

        expect(context.web3.utils.hexToNumberString(totalSnapshotAfter.totalStake)).equal(expectedStakeAfter.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshotAfter.activeStake)).equal(expectedStakeAfter.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshotAfter.totalVotingPower)).equal(expectedStakeAfter.toFixed());
        expect(context.web3.utils.hexToNumberString(totalSnapshotAfter.activeVotingPower)).equal(expectedStakeAfter.toFixed());

        // Increase ValidatorSnapshot total amount
        expect(context.web3.utils.hexToNumberString(atStakeBefore.total)).equal(expectedStakeBefore.toFixed());
        expect(context.web3.utils.hexToNumberString(atStakeAfter.total)).equal(expectedStakeAfter.toFixed());

        // Increase ValidatorSnapshot nominations`s given nominator`s amount
        expect(atStakeBefore.nominations[1].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
        expect(context.web3.utils.hexToNumberString(atStakeBefore.nominations[1].amount)).equal(expectAmount.toFixed());

        expect(atStakeAfter.nominations[1].owner.toString().toLowerCase()).equal(baltathar.address.toLowerCase());
        expect(context.web3.utils.hexToNumberString(atStakeAfter.nominations[1].amount)).equal(extraTotalstake.toFixed());

    });

});