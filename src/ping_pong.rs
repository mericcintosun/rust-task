#![no_std]

use alloc::string::ToString; 
use multiversx_sc::errors::SCError;
use multiversx_sc::imports::*;

#[derive(PartialEq, Eq, Debug)]
pub enum PingPongError {
    AlreadyPinged,                 // 0
    NoPingFound,                   // 1
    InvalidPaymentToken,           // 2
    IncorrectPingAmount,           // 3
    CannotPongBeforeDeadline,      // 4
    DurationCannotBeZero,          // 5
    PingAmountCannotBeZero,        // 6
    OnlyOwnerCanPerformThisAction, // 7
}

impl From<PingPongError> for SCError {
    fn from(error: PingPongError) -> Self {
        match error {
            PingPongError::AlreadyPinged => SCError::Custom(0),
            PingPongError::NoPingFound => SCError::Custom(1),
            PingPongError::InvalidPaymentToken => SCError::Custom(2),
            PingPongError::IncorrectPingAmount => SCError::Custom(3),
            PingPongError::CannotPongBeforeDeadline => SCError::Custom(4),
            PingPongError::DurationCannotBeZero => SCError::Custom(5),
            PingPongError::PingAmountCannotBeZero => SCError::Custom(6),
            PingPongError::OnlyOwnerCanPerformThisAction => SCError::Custom(7),
        }
    }
}

#[multiversx_sc::contract]
pub trait PingPong {

    #[storage_mapper("acceptedPaymentTokenId")]
    fn accepted_payment_token_id(&self) -> SingleValueMapper<EgldOrEsdtTokenIdentifier>;

    #[storage_mapper("pingAmount")]
    fn ping_amount(&self) -> SingleValueMapper<BigUint>;

    #[storage_mapper("durationInSeconds")]
    fn duration_in_seconds(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("userPingTimestamp")]
    fn user_ping_timestamp(&self, address: &ManagedAddress) -> SingleValueMapper<u64>;

    #[storage_mapper("paused")]
    fn paused(&self) -> SingleValueMapper<bool>;

    #[storage_mapper("owner")]
    fn owner(&self) -> SingleValueMapper<ManagedAddress>;


    #[event("pongEvent")]
    fn pong_event(&self, #[indexed] user: &ManagedAddress);

    #[event("pingEvent")]
    fn ping_event(&self, #[indexed] user: &ManagedAddress);

    #[init]
    fn init(
        &self,
        ping_amount: BigUint,
        duration_in_seconds: u64,
        opt_token_id: OptionalValue<EgldOrEsdtTokenIdentifier>,
    ) {
        require!(ping_amount > 0, PingPongError::PingAmountCannotBeZero);
        self.ping_amount().set(&ping_amount);

        require!(duration_in_seconds > 0, PingPongError::DurationCannotBeZero);
        self.duration_in_seconds().set(duration_in_seconds);

        let token_id = match opt_token_id {
            OptionalValue::Some(t) => t,
            OptionalValue::None => EgldOrEsdtTokenIdentifier::egld(),
        };
        self.accepted_payment_token_id().set(&token_id);

        let caller = self.blockchain().get_caller();
        self.owner().set(&caller);

        self.paused().set(false);
    }

    #[upgrade]
    fn upgrade(&self, ping_amount: BigUint, duration_in_seconds: u64) {
        let caller = self.blockchain().get_caller();
        require!(
            caller == self.owner().get(),
            PingPongError::OnlyOwnerCanPerformThisAction
        );

        // Yeni ping miktarını ve süreyi ayarlayın
        require!(ping_amount > 0, PingPongError::PingAmountCannotBeZero);
        self.ping_amount().set(&ping_amount);

        require!(duration_in_seconds > 0, PingPongError::DurationCannotBeZero);
        self.duration_in_seconds().set(duration_in_seconds);
    }

    #[payable("*")]
    #[endpoint]
    fn ping(&self) {
        require!(!self.paused().get(), SCError::Custom(8)); // "Contract is paused"

        let (payment_token, payment_amount) = self.call_value().egld_or_single_fungible_esdt();
        require!(
            payment_token == self.accepted_payment_token_id().get(),
            PingPongError::InvalidPaymentToken
        );
        require!(
            payment_amount == self.ping_amount().get(),
            PingPongError::IncorrectPingAmount
        );

        let caller = self.blockchain().get_caller();
        require!(!self.did_user_ping(&caller), PingPongError::AlreadyPinged);

        let current_block_timestamp = self.blockchain().get_block_timestamp();
        self.user_ping_timestamp(&caller)
            .set(current_block_timestamp);


        self.ping_event(&caller);
    }


    #[endpoint]
    fn pong(&self) {
        require!(!self.paused().get(), SCError::Custom(8)); // "Contract is paused"

        let caller = self.blockchain().get_caller();
        require!(self.did_user_ping(&caller), PingPongError::NoPingFound);

        let pong_enable_timestamp = self.get_pong_enable_timestamp(&caller);
        let current_timestamp = self.blockchain().get_block_timestamp();
        require!(
            current_timestamp >= pong_enable_timestamp,
            PingPongError::CannotPongBeforeDeadline
        );

        self.user_ping_timestamp(&caller).clear();

        let token_id = self.accepted_payment_token_id().get();
        let amount = self.ping_amount().get();

        self.send().direct(&caller, &token_id, 0, &amount);
        self.pong_event(&caller);
    }


    #[endpoint]
    fn pause(&self) {
        let caller = self.blockchain().get_caller();
        require!(
            caller == self.owner().get(),
            PingPongError::OnlyOwnerCanPerformThisAction
        );
        self.paused().set(true);
    }


    #[endpoint]
    fn unpause(&self) {
        let caller = self.blockchain().get_caller();
        require!(
            caller == self.owner().get(),
            PingPongError::OnlyOwnerCanPerformThisAction
        );
        self.paused().set(false);
    }

    #[endpoint]
    fn extend_ping_duration(&self, additional_seconds: u64) {
        require!(!self.paused().get(), SCError::Custom(8)); // "Contract is paused"

        let caller = self.blockchain().get_caller();
        require!(self.did_user_ping(&caller), PingPongError::NoPingFound);
        require!(additional_seconds > 0, SCError::Custom(9)); // "Additional seconds must be greater than zero"

        let current_pong_enable_timestamp = self.get_pong_enable_timestamp(&caller);
        let new_pong_enable_timestamp = current_pong_enable_timestamp + additional_seconds;
        self.user_ping_timestamp(&caller)
            .set(new_pong_enable_timestamp);
    }



    #[view(didUserPing)]
    fn did_user_ping(&self, address: &ManagedAddress) -> bool {
        !self.user_ping_timestamp(address).is_empty()
    }

    #[view(getPongEnableTimestamp)]
    fn get_pong_enable_timestamp(&self, address: &ManagedAddress) -> u64 {
        if !self.did_user_ping(address) {
            return 0;
        }

        let user_ping_timestamp = self.user_ping_timestamp(address).get();
        let duration_in_seconds = self.duration_in_seconds().get();

        user_ping_timestamp + duration_in_seconds
    }


    #[view(getTimeToPong)]
    fn get_time_to_pong(&self, address: &ManagedAddress) -> OptionalValue<u64> {
        if !self.did_user_ping(address) {
            return OptionalValue::None;
        }

        let pong_enable_timestamp = self.get_pong_enable_timestamp(address);
        let current_timestamp = self.blockchain().get_block_timestamp();

        if current_timestamp >= pong_enable_timestamp {
            OptionalValue::Some(0)
        } else {
            let time_left = pong_enable_timestamp - current_timestamp;
            OptionalValue::Some(time_left)
        }
    }


    #[view(getAcceptedPaymentToken)]
    fn get_accepted_payment_token(&self) -> EgldOrEsdtTokenIdentifier {
        self.accepted_payment_token_id().get()
    }

    #[view(getPingAmount)]
    fn get_ping_amount(&self) -> BigUint {
        self.ping_amount().get()
    }

    #[view(getDurationTimestamp)]
    fn get_duration_timestamp(&self) -> u64 {
        self.duration_in_seconds().get()
    }


    #[view(getUserPingTimestamp)]
    fn get_user_ping_timestamp(&self, address: &ManagedAddress) -> u64 {
        self.user_ping_timestamp(address).get()
    }


    #[view(getPaused)]
    fn get_paused(&self) -> bool {
        self.paused().get()
    }

    #[view(getOwner)]
    fn get_owner(&self) -> ManagedAddress {
        self.owner().get()
    }
}
