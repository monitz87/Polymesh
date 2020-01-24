//! # Asset Module
//!
//! The Asset module is one place to create the security tokens on the Polymesh blockchain.
//! It consist every required functionality related to securityToken and every function
//! execution can be differentiate at the token level by providing the ticker of the token.
//! In ethereum analogy every token has different smart contract address which act as the unique identity
//! of the token while here token lives at low-level where token ticker act as the differentiator
//!
//! ## Overview
//!
//! The Asset module provides functions for:
//!
//! - Creating the tokens
//! - Creation of checkpoints on the token level
//! - Management of the token (Document mgt etc)
//! - Transfer/redeem functionality of the token
//! - Custodian functionality
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `register_ticker` - Used to either register a new ticker or extend registration of an existing ticker
//! - `accept_ticker_transfer` - Used to accept a ticker transfer authorization
//! - `create_token` - Initializes a new security token
//! - `transfer` - Transfer tokens from one DID to another DID as tokens are stored/managed on the DID level
//! - `controller_transfer` - Forces a transfer between two DIDs.
//! - `approve` - Approve token transfer from one DID to DID
//! - `transfer_from` - If sufficient allowance provided, transfer from a DID to another DID without token owner's signature.
//! - `create_checkpoint` - Function used to create the checkpoint
//! - `issue` - Function is used to issue(or mint) new tokens for the given DID
//! - `batch_issue` - Batch version of issue function
//! - `redeem` - Used to redeem the security tokens
//! - `redeem_from` - Used to redeem the security tokens by some other DID who has approval
//! - `controller_redeem` - Forces a redemption of an DID's tokens. Can only be called by token owner
//! - `make_divisible` - Change the divisibility of the token to divisible. Only called by the token owner
//! - `can_transfer` - Checks whether a transaction with given parameters can take place or not
//! - `transfer_with_data` - This function can be used by the exchanges of other third parties to dynamically validate the transaction by passing the data blob
//! - `transfer_from_with_data` - This function can be used by the exchanges of other third parties to dynamically validate the transaction by passing the data blob
//! - `is_issuable` - Used to know whether the given token will issue new tokens or not
//! - `get_document` - Used to get the documents details attach with the token
//! - `set_document` - Used to set the details of the document, Only be called by the token owner
//! - `remove_document` - Used to remove the document details for the given token, Only be called by the token owner
//! - `increase_custody_allowance` - Used to increase the allowance for a given custodian
//! - `increase_custody_allowance_of` - Used to increase the allowance for a given custodian by providing the off chain signature
//! - `transfer_by_custodian` - Used to transfer the tokens by the approved custodian
//!
//! ### Public Functions
//!
//! - `is_ticker_available` - Returns if ticker is available to register
//! - `is_ticker_registry_valid` - Returns if ticker is registered to a particular did
//! - `token_details` - Returns details of the token
//! - `balance_of` - Returns the balance of the DID corresponds to the ticker
//! - `total_checkpoints_of` - Returns the checkpoint Id
//! - `total_supply_at` - Returns the total supply at a given checkpoint
//! - `custodian_allowance`- Returns the allowance provided to a custodian for a given ticker and token holder
//! - `total_custody_allowance` - Returns the total allowance approved by the token holder.

use crate::{balances, constants::*, general_tm, identity, percentage_tm, statistics, utils};
use codec::Encode;
use core::result::Result as StdResult;
use currency::*;
use primitives::{AuthorizationData, AuthorizationError, IdentityId, Key, Signer};
use rstd::{convert::TryFrom, prelude::*};
use session;
use sr_primitives::traits::{CheckedAdd, CheckedSub, Verify};
#[cfg(feature = "std")]
use sr_primitives::{Deserialize, Serialize};
use srml_support::{
    decl_event, decl_module, decl_storage,
    dispatch::Result,
    ensure,
    traits::{Currency, ExistenceRequirement, WithdrawReason},
};
use system::{self, ensure_signed};

/// The module's configuration trait.
pub trait Trait:
    system::Trait
    + general_tm::Trait
    + percentage_tm::Trait
    + utils::Trait
    + balances::Trait
    + identity::Trait
    + session::Trait
    + statistics::Trait
{
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
    type Currency: Currency<Self::AccountId>;
}

/// struct to store the token details
#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Debug)]
pub struct SecurityToken<U> {
    pub name: Vec<u8>,
    pub total_supply: U,
    pub owner_did: IdentityId,
    pub divisible: bool,
}

/// struct to store the signed data
#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Debug)]
pub struct SignData<U> {
    custodian_did: IdentityId,
    holder_did: IdentityId,
    ticker: Vec<u8>,
    value: U,
    nonce: u16,
}

/// struct to store the ticker registration details
#[derive(codec::Encode, codec::Decode, Clone, Default, PartialEq, Debug)]
pub struct TickerRegistration<U> {
    owner: IdentityId,
    expiry: Option<U>,
}

/// struct to store the ticker registration config
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(codec::Encode, codec::Decode, Clone, Default, PartialEq, Debug)]
pub struct TickerRegistrationConfig<U> {
    pub max_ticker_length: u32,
    pub registration_length: Option<U>,
}

/// struct to store the ticker transfer approvals
#[derive(codec::Encode, codec::Decode, Clone, Default, PartialEq, Debug)]
pub struct TickerTransferApproval<U> {
    pub authorized_by: U,
    pub next_ticker: Option<Vec<u8>>,
    pub previous_ticker: Option<Vec<u8>>,
}

#[derive(codec::Encode, codec::Decode, Clone, Eq, PartialEq, Debug)]
pub enum TickerRegistrationStatus {
    RegisteredByOther,
    Available,
    RegisteredByDid,
}

decl_storage! {
    trait Store for Module<T: Trait> as Asset {
        /// The DID of the fee collector
        FeeCollector get(fee_collector) config(): T::AccountId;
        /// Ticker registration details
        /// (ticker) -> TickerRegistration
        pub Tickers get(ticker_registration): map Vec<u8> => TickerRegistration<T::Moment>;
        /// Ticker registration config
        /// (ticker) -> TickerRegistrationConfig
        pub TickerConfig get(ticker_registration_config) config(): TickerRegistrationConfig<T::Moment>;
        /// details of the token corresponding to the token ticker
        /// (ticker) -> SecurityToken details [returns SecurityToken struct]
        pub Tokens get(token_details): map Vec<u8> => SecurityToken<T::Balance>;
        /// Used to store the securityToken balance corresponds to ticker and Identity
        /// (ticker, DID) -> balance
        pub BalanceOf get(balance_of): map (Vec<u8>, IdentityId) => T::Balance;
        /// (ticker, sender (DID), spender(DID)) -> allowance amount
        Allowance get(allowance): map (Vec<u8>, IdentityId, IdentityId) => T::Balance;
        /// cost in base currency to create a token
        AssetCreationFee get(asset_creation_fee) config(): T::Balance;
        /// cost in base currency to register a ticker
        TickerRegistrationFee get(ticker_registration_fee) config(): T::Balance;
        /// Checkpoints created per token
        /// (ticker) -> no. of checkpoints
        pub TotalCheckpoints get(total_checkpoints_of): map Vec<u8> => u64;
        /// Total supply of the token at the checkpoint
        /// (ticker, checkpointId) -> total supply at given checkpoint
        pub CheckpointTotalSupply get(total_supply_at): map (Vec<u8>, u64) => T::Balance;
        /// Balance of a DID at a checkpoint
        /// (ticker, DID, checkpoint ID) -> Balance of a DID at a checkpoint
        CheckpointBalance get(balance_at_checkpoint): map (Vec<u8>, IdentityId, u64) => T::Balance;
        /// Last checkpoint updated for a DID's balance
        /// (ticker, DID) -> List of checkpoints where user balance changed
        UserCheckpoints get(user_checkpoints): map (Vec<u8>, IdentityId) => Vec<u64>;
        /// The documents attached to the tokens
        /// (ticker, document name) -> (URI, document hash)
        Documents get(documents): map (Vec<u8>, Vec<u8>) => (Vec<u8>, Vec<u8>, T::Moment);
        /// Allowance provided to the custodian
        /// (ticker, token holder, custodian) -> balance
        pub CustodianAllowance get(custodian_allowance): map(Vec<u8>, IdentityId, IdentityId) => T::Balance;
        /// Total custodian allowance for a given token holder
        /// (ticker, token holder) -> balance
        pub TotalCustodyAllowance get(total_custody_allowance): map(Vec<u8>, IdentityId) => T::Balance;
        /// Store the nonce for off chain signature to increase the custody allowance
        /// (ticker, token holder, nonce) -> bool
        AuthenticationNonce get(authentication_nonce): map(Vec<u8>, IdentityId, u16) => bool;
    }
}

// public interface for this runtime module
decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        /// initialize the default event for this module
        fn deposit_event() = default;

        /// This function is used to either register a new ticker or extend validity of an exisitng ticker
        /// NB Ticker validity does not get carryforward when renewing ticker
        ///
        /// # Arguments
        /// * `origin` It consist the signing key of the caller (i.e who signed the transaction to execute this function)
        /// * `_ticker` ticker to register
        pub fn register_ticker(origin, _ticker: Vec<u8>) -> Result {
            let sender = ensure_signed(origin)?;
            let sender_key = Key::try_from(sender.encode())?;
            let signer = Signer::Key( sender_key.clone());
            let to_did =  match <identity::Module<T>>::current_did() {
                Some(x) => x,
                None => {
                    if let Some(did) = <identity::Module<T>>::get_identity(&sender_key) {
                        did
                    } else {
                        return Err("did not found");
                    }
                }
            };

            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            ensure!(<identity::Module<T>>::is_signer_authorized(to_did, &signer), "sender must be a signing key for DID");

            ensure!(!<Tokens<T>>::exists(&ticker), "token already created");

            let ticker_config = Self::ticker_registration_config();

            ensure!(ticker.len() <= usize::try_from(ticker_config.max_ticker_length).unwrap_or_default(), "ticker length over the limit");

            // Ensure that the ticker is not registered by someone else
            ensure!(
                Self::is_ticker_available_or_registered_to(&ticker, to_did) != TickerRegistrationStatus::RegisteredByOther,
                "ticker registered to someone else"
            );

            let now = <timestamp::Module<T>>::get();
            let expiry = if let Some(exp) = ticker_config.registration_length { Some(now + exp) } else { None };

            Self::_register_ticker(&ticker, sender, to_did, expiry);

            Ok(())
        }

        /// This function is used to accept a ticker transfer
        /// NB: To reject the transfer, call remove auth function in identity module.
        ///
        /// # Arguments
        /// * `origin` It consist the signing key of the caller (i.e who signed the transaction to execute this function)
        /// * `auth_id` Authorization ID of ticker transfer authorization
        pub fn accept_ticker_transfer(origin, auth_id: u64) -> Result {
            let sender = ensure_signed(origin)?;
            let sender_key = Key::try_from(sender.encode())?;
            let to_did =  match <identity::Module<T>>::current_did() {
                Some(x) => x,
                None => {
                    if let Some(did) = <identity::Module<T>>::get_identity(&sender_key) {
                        did
                    } else {
                        return Err("did not found");
                    }
                }
            };
            Self::_accept_ticker_transfer(to_did, auth_id)
        }

        /// Initializes a new security token
        /// makes the initiating account the owner of the security token
        /// & the balance of the owner is set to total supply
        ///
        /// # Arguments
        /// * `origin` It consist the signing key of the caller (i.e who signed the transaction to execute this function)
        /// * `did` DID of the creator of the token or the owner of the token
        /// * `name` Name of the token
        /// * `_ticker` Symbol of the token
        /// * `total_supply` Total supply of the token
        /// * `divisible` boolean to identify the divisibility status of the token.
        pub fn create_token(origin, did: IdentityId, name: Vec<u8>, _ticker: Vec<u8>, total_supply: T::Balance, divisible: bool) -> Result {
            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            ensure!(!<Tokens<T>>::exists(&ticker), "token already created");

            let ticker_config = Self::ticker_registration_config();

            ensure!(ticker.len() <= usize::try_from(ticker_config.max_ticker_length).unwrap_or_default(), "ticker length over the limit");

            // checking max size for name and ticker
            // byte arrays (vecs) with no max size should be avoided
            ensure!(name.len() <= 64, "token name cannot exceed 64 bytes");

            let is_ticker_available_or_registered_to = Self::is_ticker_available_or_registered_to(&ticker, did);

            ensure!(is_ticker_available_or_registered_to != TickerRegistrationStatus::RegisteredByOther, "Ticker registered to someone else");

            if !divisible {
                ensure!(total_supply % ONE_UNIT.into() == 0.into(), "Invalid Total supply");
            }

            ensure!(total_supply <= MAX_SUPPLY.into(), "Total supply above the limit");

            // Alternative way to take a fee - fee is proportionaly paid to the validators and dust is burned
            let validators = <session::Module<T>>::validators();
            let fee = Self::asset_creation_fee();
            let validator_len:T::Balance;
            if validators.len() < 1 {
                validator_len = T::Balance::from(1 as u32);
            } else {
                validator_len = T::Balance::from(validators.len() as u32);
            }
            let proportional_fee = fee / validator_len;
            for v in validators {
                <balances::Module<T> as Currency<_>>::transfer(
                    &sender,
                    &<T as utils::Trait>::validator_id_to_account_id(v),
                    proportional_fee
                )?;
            }
            let remainder_fee = fee - (proportional_fee * validator_len);
            let _withdraw_result = <balances::Module<T>>::withdraw(&sender, remainder_fee, WithdrawReason::Fee, ExistenceRequirement::KeepAlive)?;

            if is_ticker_available_or_registered_to == TickerRegistrationStatus::Available {
                // ticker not registered by anyone (or registry expired). we can charge fee and register this ticker
                Self::_register_ticker(&ticker, sender, did, None);
            } else {
                // Ticker already registered by the user
                <Tickers<T>>::mutate(&ticker, |tr| tr.expiry = None);
            }

            let token = SecurityToken {
                name,
                total_supply,
                owner_did: did,
                divisible: divisible
            };

            <Tokens<T>>::insert(&ticker, token);
            <BalanceOf<T>>::insert((ticker.clone(), did), total_supply);
            Self::deposit_event(RawEvent::IssuedToken(ticker, total_supply, did, divisible));

            Ok(())
        }

        /// Transfer tokens from one DID to another DID as tokens are stored/managed on the DID level
        ///
        /// # Arguments
        /// * `_origin` signing key of the sender
        /// * `did` DID of the `from` token holder, from whom tokens needs to transferred
        /// * `_ticker` Ticker of the token
        /// * `to_did` DID of the `to` token holder, to whom token needs to transferred
        /// * `value` Value that needs to transferred
        pub fn transfer(_origin, did: IdentityId, _ticker: Vec<u8>, to_did: IdentityId, value: T::Balance) -> Result {
            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(_origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);


            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            // Check whether the custody allowance remain intact or not
            Self::_check_custody_allowance(&ticker, did, value)?;
            ensure!(Self::_is_valid_transfer(&ticker, Some(did), Some(to_did), value)? == ERC1400_TRANSFER_SUCCESS, "Transfer restrictions failed");

            Self::_transfer(&ticker, did, to_did, value)
        }

        /// Forces a transfer between two DIDs & This can only be called by security token owner.
        /// This function doesn't validate any type of restriction beside a valid KYC check
        ///
        /// # Arguments
        /// * `_origin` signing key of the token owner DID.
        /// * `did` Token owner DID.
        /// * `_ticker` symbol of the token
        /// * `from_did` DID of the token holder from whom balance token will be transferred.
        /// * `to_did` DID of token holder to whom token balance will be transferred.
        /// * `value` Amount of tokens.
        /// * `data` Some off chain data to validate the restriction.
        /// * `operator_data` It is a string which describes the reason of this control transfer call.
        pub fn controller_transfer(_origin, did: IdentityId, _ticker: Vec<u8>, from_did: IdentityId, to_did: IdentityId, value: T::Balance, data: Vec<u8>, operator_data: Vec<u8>) -> Result {
            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(_origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            ensure!(Self::is_owner(&ticker, did), "user is not authorized");

            Self::_transfer(&ticker, from_did, to_did, value.clone())?;

            Self::deposit_event(RawEvent::ControllerTransfer(ticker, did, from_did, to_did, value, data, operator_data));

            Ok(())
        }

        /// approve token transfer from one DID to DID
        /// once this is done, transfer_from can be called with corresponding values
        ///
        /// # Arguments
        /// * `_origin` Signing key of the token owner (i.e sender)
        /// * `did` DID of the sender
        /// * `spender_did` DID of the spender
        /// * `value` Amount of the tokens approved
        fn approve(_origin, did: IdentityId, _ticker: Vec<u8>, spender_did: IdentityId, value: T::Balance) -> Result {
            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(_origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            ensure!(<BalanceOf<T>>::exists((ticker.clone(), did)), "Account does not own this token");

            let allowance = Self::allowance((ticker.clone(), did, spender_did));
            let updated_allowance = allowance.checked_add(&value).ok_or("overflow in calculating allowance")?;
            <Allowance<T>>::insert((ticker.clone(), did, spender_did), updated_allowance);

            Self::deposit_event(RawEvent::Approval(ticker, did, spender_did, value));

            Ok(())
        }

        /// If sufficient allowance provided, transfer from a DID to another DID without token owner's signature.
        ///
        /// # Arguments
        /// * `_origin` Signing key of spender
        /// * `did` DID of the spender
        /// * `_ticker` Ticker of the token
        /// * `from_did` DID from whom token is being transferred
        /// * `to_did` DID to whom token is being transferred
        /// * `value` Amount of the token for transfer
        pub fn transfer_from(origin, did: IdentityId, _ticker: Vec<u8>, from_did: IdentityId, to_did: IdentityId, value: T::Balance) -> Result {
            let spender = Signer::Key( Key::try_from( ensure_signed(origin)?.encode())?);

            // Check that spender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &spender), "sender must be a signing key for DID");

            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let ticker_from_did_did = (ticker.clone(), from_did, did);
            ensure!(<Allowance<T>>::exists(&ticker_from_did_did), "Allowance does not exist");
            let allowance = Self::allowance(&ticker_from_did_did);
            ensure!(allowance >= value, "Not enough allowance");

            // using checked_sub (safe math) to avoid overflow
            let updated_allowance = allowance.checked_sub(&value).ok_or("overflow in calculating allowance")?;
            // Check whether the custody allowance remain intact or not
            Self::_check_custody_allowance(&ticker, from_did, value)?;

            ensure!(Self::_is_valid_transfer(&ticker, Some(from_did), Some(to_did), value)? == ERC1400_TRANSFER_SUCCESS, "Transfer restrictions failed");
            Self::_transfer(&ticker, from_did, to_did, value)?;

            // Change allowance afterwards
            <Allowance<T>>::insert(&ticker_from_did_did, updated_allowance);

            Self::deposit_event(RawEvent::Approval(ticker, from_did, did, value));
            Ok(())
        }

        /// Function used to create the checkpoint
        ///
        /// # Arguments
        /// * `_origin` Signing key of the token owner. (Only token owner can call this function).
        /// * `did` DID of the token owner
        /// * `_ticker` Ticker of the token
        pub fn create_checkpoint(_origin, did: IdentityId, _ticker: Vec<u8>) -> Result {
            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(_origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            ensure!(Self::is_owner(&ticker, did), "user is not authorized");
            Self::_create_checkpoint(&ticker)
        }

        /// Function is used to issue(or mint) new tokens for the given DID
        /// can only be executed by the token owner
        ///
        /// # Arguments
        /// * `origin` Signing key of token owner
        /// * `did` DID of the token owner
        /// * `ticker` Ticker of the token
        /// * `to_did` DID of the token holder to whom new tokens get issued.
        /// * `value` Amount of tokens that get issued
        pub fn issue(origin, did: IdentityId, ticker: Vec<u8>, to_did: IdentityId, value: T::Balance, _data: Vec<u8>) -> Result {
            let upper_ticker = utils::bytes_to_upper(&ticker);
            let sender = ensure_signed(origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            ensure!(Self::is_owner(&upper_ticker, did), "user is not authorized");
            Self::_mint(&upper_ticker, to_did, value)
        }

        /// Function is used issue(or mint) new tokens for the given DIDs
        /// can only be executed by the token owner
        ///
        /// # Arguments
        /// * `origin` Signing key of token owner
        /// * `did` DID of the token owner
        /// * `ticker` Ticker of the token
        /// * `investor_dids` Array of the DID of the token holders to whom new tokens get issued.
        /// * `values` Array of the Amount of tokens that get issued
        pub fn batch_issue(origin, did: IdentityId, ticker: Vec<u8>, investor_dids: Vec<IdentityId>, values: Vec<T::Balance>) -> Result {
            let sender = ensure_signed(origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            ensure!(investor_dids.len() == values.len(), "Investor/amount list length inconsistent");

            ensure!(Self::is_owner(&ticker, did), "user is not authorized");


            // A helper vec for calculated new investor balances
            let mut updated_balances = Vec::with_capacity(investor_dids.len());

            // A helper vec for calculated new investor balances
            let mut current_balances = Vec::with_capacity(investor_dids.len());

            // Get current token details for supply update
            let mut token = Self::token_details(ticker.clone());

            // A round of per-investor checks
            for i in 0..investor_dids.len() {
                ensure!(
                    Self::check_granularity(&ticker, values[i]),
                    "Invalid granularity"
                );
                let updated_total_supply = token
                    .total_supply
                    .checked_add(&values[i])
                    .ok_or("overflow in calculating total supply")?;
                ensure!(updated_total_supply <= MAX_SUPPLY.into(), "Total supply above the limit");

                current_balances.push(Self::balance_of((ticker.clone(), investor_dids[i].clone())));
                updated_balances.push(current_balances[i]
                    .checked_add(&values[i])
                    .ok_or("overflow in calculating balance")?);

                // verify transfer check
                ensure!(Self::_is_valid_transfer(&ticker, None, Some(investor_dids[i]), values[i])? == ERC1400_TRANSFER_SUCCESS, "Transfer restrictions failed");

                // New total supply must be valid
                token.total_supply = updated_total_supply;
            }

            // After checks are ensured introduce side effects
            for i in 0..investor_dids.len() {
                Self::_update_checkpoint(&ticker, investor_dids[i], current_balances[i]);

                <BalanceOf<T>>::insert((ticker.clone(), investor_dids[i]), updated_balances[i]);
                <statistics::Module<T>>::update_transfer_stats( &ticker, None, Some(updated_balances[i]), values[i]);

                Self::deposit_event(RawEvent::Issued(ticker.clone(), investor_dids[i], values[i]));
            }
            <Tokens<T>>::insert(ticker.clone(), token);

            Ok(())
        }

        /// Used to redeem the security tokens
        ///
        /// # Arguments
        /// * `_origin` Signing key of the token holder who wants to redeem the tokens
        /// * `did` DID of the token holder
        /// * `_ticker` Ticker of the token
        /// * `value` Amount of the tokens needs to redeem
        /// * `_data` An off chain data blob used to validate the redeem functionality.
        pub fn redeem(_origin, did: IdentityId, _ticker: Vec<u8>, value: T::Balance, _data: Vec<u8>) -> Result {
            let upper_ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(_origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            // Granularity check
            ensure!(
                Self::check_granularity(&upper_ticker, value),
                "Invalid granularity"
                );
            let ticker_did = (upper_ticker.clone(), did);
            ensure!(<BalanceOf<T>>::exists(&ticker_did), "Account does not own this token");
            let burner_balance = Self::balance_of(&ticker_did);
            ensure!(burner_balance >= value, "Not enough balance.");

            // Reduce sender's balance
            let updated_burner_balance = burner_balance
                .checked_sub(&value)
                .ok_or("overflow in calculating balance")?;
            // Check whether the custody allowance remain intact or not
            Self::_check_custody_allowance(&upper_ticker, did, value)?;

            // verify transfer check
            ensure!(Self::_is_valid_transfer(&upper_ticker, Some(did), None, value)? == ERC1400_TRANSFER_SUCCESS, "Transfer restrictions failed");

            //Decrease total supply
            let mut token = Self::token_details(&upper_ticker);
            token.total_supply = token.total_supply.checked_sub(&value).ok_or("overflow in calculating balance")?;

            Self::_update_checkpoint(&upper_ticker, did, burner_balance);

            <BalanceOf<T>>::insert((upper_ticker.clone(), did), updated_burner_balance);
            <Tokens<T>>::insert(&upper_ticker, token);
            <statistics::Module<T>>::update_transfer_stats( &upper_ticker, Some(updated_burner_balance), None, value);


            Self::deposit_event(RawEvent::Redeemed(upper_ticker, did, value));

            Ok(())

        }

        /// Used to redeem the security tokens by some other DID who has approval
        ///
        /// # Arguments
        /// * `_origin` Signing key of the spender who has valid approval to redeem the tokens
        /// * `did` DID of the spender
        /// * `_ticker` Ticker of the token
        /// * `from_did` DID from whom balance get reduced
        /// * `value` Amount of the tokens needs to redeem
        /// * `_data` An off chain data blob used to validate the redeem functionality.
        pub fn redeem_from(_origin, did: IdentityId, _ticker: Vec<u8>, from_did: IdentityId, value: T::Balance, _data: Vec<u8>) -> Result {
            let upper_ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(_origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");

            // Granularity check
            ensure!(
                Self::check_granularity(&upper_ticker, value),
                "Invalid granularity"
                );
            let ticker_did = (upper_ticker.clone(), did);
            ensure!(<BalanceOf<T>>::exists(&ticker_did), "Account does not own this token");
            let burner_balance = Self::balance_of(&ticker_did);
            ensure!(burner_balance >= value, "Not enough balance.");

            // Reduce sender's balance
            let updated_burner_balance = burner_balance
                .checked_sub(&value)
                .ok_or("overflow in calculating balance")?;

            let ticker_from_did_did = (upper_ticker.clone(), from_did, did);
            ensure!(<Allowance<T>>::exists(&ticker_from_did_did), "Allowance does not exist");
            let allowance = Self::allowance(&ticker_from_did_did);
            ensure!(allowance >= value, "Not enough allowance");
            // Check whether the custody allowance remain intact or not
            Self::_check_custody_allowance(&upper_ticker, did, value)?;
            ensure!(Self::_is_valid_transfer( &upper_ticker, Some(from_did), None, value)? == ERC1400_TRANSFER_SUCCESS, "Transfer restrictions failed");

            let updated_allowance = allowance.checked_sub(&value).ok_or("overflow in calculating allowance")?;

            //Decrease total suply
            let mut token = Self::token_details(&upper_ticker);
            token.total_supply = token.total_supply.checked_sub(&value).ok_or("overflow in calculating balance")?;

            Self::_update_checkpoint(&upper_ticker, did, burner_balance);

            <Allowance<T>>::insert(&ticker_from_did_did, updated_allowance);
            <BalanceOf<T>>::insert(&ticker_did, updated_burner_balance);
            <Tokens<T>>::insert(&upper_ticker, token);
            <statistics::Module<T>>::update_transfer_stats( &upper_ticker, Some(updated_burner_balance), None, value);

            Self::deposit_event(RawEvent::Redeemed(upper_ticker.clone(), did, value));
            Self::deposit_event(RawEvent::Approval(upper_ticker, from_did, did, value));

            Ok(())
        }

        /// Forces a redemption of an DID's tokens. Can only be called by token owner
        ///
        /// # Arguments
        /// * `_origin` Signing key of the token owner
        /// * `did` DID of the token holder
        /// * `ticker` Ticker of the token
        /// * `token_holder_did` DID from whom balance get reduced
        /// * `value` Amount of the tokens needs to redeem
        /// * `data` An off chain data blob used to validate the redeem functionality.
        /// * `operator_data` Any data blob that defines the reason behind the force redeem.
        pub fn controller_redeem(origin, did: IdentityId, ticker: Vec<u8>, token_holder_did: IdentityId, value: T::Balance, data: Vec<u8>, operator_data: Vec<u8>) -> Result {
            let ticker = utils::bytes_to_upper(ticker.as_slice());
            let sender = ensure_signed(origin)?;
            let signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &signer), "sender must be a signing key for DID");
            ensure!(Self::is_owner(&ticker, did), "user is not token owner");

            // Granularity check
            ensure!(
                Self::check_granularity(&ticker, value),
                "Invalid granularity"
                );
            let ticker_token_holder_did = (ticker.clone(), token_holder_did);
            ensure!(<BalanceOf<T>>::exists( &ticker_token_holder_did), "Account does not own this token");
            let burner_balance = Self::balance_of(&ticker_token_holder_did);
            ensure!(burner_balance >= value, "Not enough balance.");

            // Reduce sender's balance
            let updated_burner_balance = burner_balance
                .checked_sub(&value)
                .ok_or("overflow in calculating balance")?;

            //Decrease total suply
            let mut token = Self::token_details(&ticker);
            token.total_supply = token.total_supply.checked_sub(&value).ok_or("overflow in calculating balance")?;

            Self::_update_checkpoint(&ticker, token_holder_did, burner_balance);

            <BalanceOf<T>>::insert(&ticker_token_holder_did, updated_burner_balance);
            <Tokens<T>>::insert(&ticker, token);
            <statistics::Module<T>>::update_transfer_stats( &ticker, Some(updated_burner_balance), None, value);

            Self::deposit_event(RawEvent::ControllerRedemption(ticker, did, token_holder_did, value, data, operator_data));

            Ok(())
        }

        /// Makes an indivisible token divisible. Only called by the token owner
        ///
        /// # Arguments
        /// * `origin` Signing key of the token owner.
        /// * `did` DID of the token owner
        /// * `ticker` Ticker of the token
        pub fn make_divisible(origin, did: IdentityId, ticker: Vec<u8>) -> Result {
            let ticker = utils::bytes_to_upper(ticker.as_slice());
            let sender = ensure_signed(origin)?;
            let sender_signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &sender_signer), "sender must be a signing key for DID");

            ensure!(Self::is_owner(&ticker, did), "user is not authorized");
            // Read the token details
            let mut token = Self::token_details(&ticker);
            ensure!(!token.divisible, "token already divisible");
            token.divisible = true;
            <Tokens<T>>::insert(&ticker, token);
            Self::deposit_event(RawEvent::DivisibilityChanged(ticker, true));
            Ok(())
        }

        /// Checks whether a transaction with given parameters can take place or not
        /// This function is state less function and used to validate the transfer before actual transfer call.
        ///
        /// # Arguments
        /// * `_origin` Signing Key of the caller
        /// * `ticker` Ticker of the token
        /// * `from_did` DID from whom tokens will be transferred
        /// * `to_did` DID to whom tokens will be transferred
        /// * `value` Amount of the tokens
        /// * `data` Off chain data blob to validate the transfer.
        pub fn can_transfer(_origin, ticker: Vec<u8>, from_did: IdentityId, to_did: IdentityId, value: T::Balance, data: Vec<u8>) {
            let mut current_balance: T::Balance = Self::balance_of((ticker.clone(), from_did));
            if current_balance < value {
                current_balance = 0.into();
            } else {
                current_balance = current_balance - value;
            }
            if current_balance < Self::total_custody_allowance((ticker.clone(), from_did)) {
                sr_primitives::print("Insufficient balance");
                Self::deposit_event(RawEvent::CanTransfer(ticker, from_did, to_did, value, data, ERC1400_INSUFFICIENT_BALANCE as u32));
            } else {
                match Self::_is_valid_transfer(&ticker, Some(from_did), Some(to_did), value) {
                    Ok(code) =>
                    {
                        Self::deposit_event(RawEvent::CanTransfer(ticker, from_did, to_did, value, data, code as u32));
                    },
                    Err(msg) => {
                        // We emit a generic error with the event whenever there's an internal issue - i.e. captured
                        // in a string error and not using the status codes
                        sr_primitives::print(msg);
                        Self::deposit_event(RawEvent::CanTransfer(ticker, from_did, to_did, value, data, ERC1400_TRANSFER_FAILURE as u32));
                    }
                }
            }
        }

        /// An ERC1594 transfer with data
        /// This function can be used by the exchanges of other third parties to dynamically validate the transaction
        /// by passing the data blob
        ///
        /// # Arguments
        /// * `origin` Signing key of the sender
        /// * `did` DID from whom tokens will be transferred
        /// * `ticker` Ticker of the token
        /// * `to_did` DID to whom tokens will be transferred
        /// * `value` Amount of the tokens
        /// * `data` Off chain data blob to validate the transfer.
        pub fn transfer_with_data(origin, did: IdentityId, ticker: Vec<u8>, to_did: IdentityId, value: T::Balance, data: Vec<u8>) -> Result {
            Self::transfer(origin, did, ticker.clone(), to_did, value)?;
            Self::deposit_event(RawEvent::TransferWithData(ticker, did, to_did, value, data));
            Ok(())
        }

        /// An ERC1594 transfer_from with data
        /// This function can be used by the exchanges of other third parties to dynamically validate the transaction
        /// by passing the data blob
        ///
        /// # Arguments
        /// * `origin` Signing key of the spender
        /// * `did` DID of spender
        /// * `ticker` Ticker of the token
        /// * `from_did` DID from whom tokens will be transferred
        /// * `to_did` DID to whom tokens will be transferred
        /// * `value` Amount of the tokens
        /// * `data` Off chain data blob to validate the transfer.
        pub fn transfer_from_with_data(origin, did: IdentityId, ticker: Vec<u8>, from_did: IdentityId, to_did: IdentityId, value: T::Balance, data: Vec<u8>) -> Result {
            Self::transfer_from(origin, did, ticker.clone(), from_did,  to_did, value)?;
            Self::deposit_event(RawEvent::TransferWithData(ticker, from_did, to_did, value, data));
            Ok(())
        }

        /// Used to know whether the given token will issue new tokens or not
        ///
        /// # Arguments
        /// * `_origin` Signing key
        /// * `ticker` Ticker of the token whose issuance status need to know
        pub fn is_issuable(_origin, ticker: Vec<u8>) {
            Self::deposit_event(RawEvent::IsIssuable(ticker, true));
        }

        /// Used to get the documents details attach with the token
        ///
        /// # Arguments
        /// * `_origin` Caller signing key
        /// * `ticker` Ticker of the token
        /// * `name` Name of the document
        pub fn get_document(_origin, ticker: Vec<u8>, name: Vec<u8>) -> Result {
            let record = <Documents<T>>::get((ticker.clone(), name.clone()));
            Self::deposit_event(RawEvent::GetDocument(ticker, name, record.0, record.1, record.2));
            Ok(())
        }

        /// Used to set the details of the document, Only be called by the token owner
        ///
        /// # Arguments
        /// * `origin` Signing key of the token owner
        /// * `did` DID of the token owner
        /// * `ticker` Ticker of the token
        /// * `name` Name of the document
        /// * `uri` Off chain URL of the document
        /// * `document_hash` Hash of the document to proof the incorruptibility of the document
        pub fn set_document(origin, did: IdentityId, ticker: Vec<u8>, name: Vec<u8>, uri: Vec<u8>, document_hash: Vec<u8>) -> Result {
            let ticker = utils::bytes_to_upper(ticker.as_slice());
            let sender = ensure_signed(origin)?;
            let sender_signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &sender_signer), "sender must be a signing key for DID");
            ensure!(Self::is_owner(&ticker, did), "user is not authorized");

            <Documents<T>>::insert((ticker, name), (uri, document_hash, <timestamp::Module<T>>::get()));
            Ok(())
        }

        /// Used to remove the document details for the given token, Only be called by the token owner
        ///
        /// # Arguments
        /// * `origin` Signing key of the token owner
        /// * `did` DID of the token owner
        /// * `ticker` Ticker of the token
        /// * `name` Name of the document
        pub fn remove_document(origin, did: IdentityId, ticker: Vec<u8>, name: Vec<u8>) -> Result {
            let ticker = utils::bytes_to_upper(ticker.as_slice());
            let sender = ensure_signed(origin)?;
            let sender_signer = Signer::Key( Key::try_from(sender.encode())?);


            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signer_authorized(did, &sender_signer), "sender must be a signing key for DID");
            ensure!(Self::is_owner(&ticker, did), "user is not authorized");

            <Documents<T>>::remove((ticker, name));
            Ok(())
        }

        /// ERC-2258 Implementation

        /// Used to increase the allowance for a given custodian
        /// Any investor/token holder can add a custodian and transfer the token transfer ownership to the custodian
        /// Through that investor balance will remain the same but the given token are only transfer by the custodian.
        /// This implementation make sure to have an accurate investor count from omnibus wallets.
        ///
        /// # Arguments
        /// * `origin` Signing key of the token holder
        /// * `ticker` Ticker of the token
        /// * `holder_did` DID of the token holder (i.e who wants to increase the custody allowance)
        /// * `custodian_did` DID of the custodian (i.e whom allowance provided)
        /// * `value` Allowance amount
        pub fn increase_custody_allowance(origin, ticker: Vec<u8>, holder_did: IdentityId, custodian_did: IdentityId, value: T::Balance) -> Result {
            let ticker = utils::bytes_to_upper(ticker.as_slice());
            let sender = ensure_signed(origin)?;
            let sender_signer = Signer::Key( Key::try_from(sender.encode())?);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(
                <identity::Module<T>>::is_signer_authorized(holder_did, &sender_signer),
                "sender must be a signing key for DID"
            );
            Self::_increase_custody_allowance(ticker.clone(), holder_did, custodian_did, value)?;
            Ok(())
        }

        /// Used to increase the allowance for a given custodian by providing the off chain signature
        ///
        /// # Arguments
        /// * `origin` Signing key of a DID who posses off chain signature
        /// * `ticker` Ticker of the token
        /// * `holder_did` DID of the token holder (i.e who wants to increase the custody allowance)
        /// * `holder_account_id` Signing key which signs the off chain data blob.
        /// * `custodian_did` DID of the custodian (i.e whom allowance provided)
        /// * `caller_did` DID of the caller
        /// * `value` Allowance amount
        /// * `nonce` A u16 number which avoid the replay attack
        /// * `signature` Signature provided by the holder_did
        pub fn increase_custody_allowance_of(
            origin,
            ticker: Vec<u8>,
            holder_did: IdentityId,
            holder_account_id: T::AccountId,
            custodian_did: IdentityId,
            caller_did: IdentityId,
            value: T::Balance,
            nonce: u16,
            signature: T::OffChainSignature
        ) -> Result {
            let ticker = utils::bytes_to_upper(ticker.as_slice());
            let sender = ensure_signed(origin)?;

            ensure!(!Self::authentication_nonce((ticker.clone(), holder_did, nonce)), "Signature already used");

            let msg = SignData {
                custodian_did: custodian_did,
                holder_did: holder_did,
                ticker: ticker.clone(),
                value,
                nonce
            };
            // holder_account_id should be a part of the holder_did
            ensure!(signature.verify(&msg.encode()[..], &holder_account_id), "Invalid signature");
            let sender_signer = Signer::Key(Key::try_from(sender.encode())?);
            ensure!(
                <identity::Module<T>>::is_signer_authorized(caller_did, &sender_signer),
                "sender must be a signing key for DID"
            );
            // Validate the holder signing key
            let holder_signer = Signer::Key(Key::try_from(holder_account_id.encode())?);
            ensure!(
                <identity::Module<T>>::is_signer_authorized(holder_did, &holder_signer),
                "holder signing key must be a signing key for holder DID"
            );
            Self::_increase_custody_allowance(ticker.clone(), holder_did, custodian_did, value)?;
            <AuthenticationNonce>::insert((ticker.clone(), holder_did, nonce), true);
            Ok(())
        }

        /// Used to transfer the tokens by the approved custodian
        ///
        /// # Arguments
        /// * `origin` Signing key of the custodian
        /// * `ticker` Ticker of the token
        /// * `holder_did` DID of the token holder (i.e whom balance get reduced)
        /// * `custodian_did` DID of the custodian (i.e who has the valid approved allowance)
        /// * `receiver_did` DID of the receiver
        /// * `value` Amount of tokens need to transfer
        pub fn transfer_by_custodian(
            origin,
            ticker: Vec<u8>,
            holder_did: IdentityId,
            custodian_did: IdentityId,
            receiver_did: IdentityId,
            value: T::Balance
        ) -> Result {
            let ticker = utils::bytes_to_upper(ticker.as_slice());
            let sender = ensure_signed(origin)?;
            let sender_signer = Signer::Key( Key::try_from(sender.encode())?);
            // Check that sender is allowed to act on behalf of `did`
            ensure!(
                <identity::Module<T>>::is_signer_authorized(custodian_did, &sender_signer),
                "sender must be a signing key for DID"
            );
            let mut custodian_allowance = Self::custodian_allowance((ticker.clone(), holder_did, custodian_did));
            // Check whether the custodian has enough allowance or not
            ensure!(custodian_allowance >= value, "Insufficient allowance");
            // using checked_sub (safe math) to avoid underflow
            custodian_allowance = custodian_allowance.checked_sub(&value).ok_or("underflow in calculating allowance")?;
            // using checked_sub (safe math) to avoid underflow
            let new_total_allowance = Self::total_custody_allowance((ticker.clone(), holder_did))
                .checked_sub(&value)
                .ok_or("underflow in calculating the total allowance")?;
            // Validate the transfer
            ensure!(Self::_is_valid_transfer(&ticker, Some(holder_did), Some(receiver_did), value)? == ERC1400_TRANSFER_SUCCESS, "Transfer restrictions failed");
            Self::_transfer(&ticker, holder_did, receiver_did, value)?;
            // Update Storage of allowance
            <CustodianAllowance<T>>::insert((ticker.clone(), custodian_did, holder_did), &custodian_allowance);
            <TotalCustodyAllowance<T>>::insert((ticker.clone(), holder_did), new_total_allowance);
            Self::deposit_event(RawEvent::CustodyTransfer(ticker.clone(), custodian_did, holder_did, receiver_did, value));
            Ok(())
        }
    }
}

decl_event! {
    pub enum Event<T>
        where
        Balance = <T as balances::Trait>::Balance,
        Moment = <T as timestamp::Trait>::Moment,
    {
        /// event for transfer of tokens
        /// ticker, from DID, to DID, value
        Transfer(Vec<u8>, IdentityId, IdentityId, Balance),
        /// event when an approval is made
        /// ticker, owner DID, spender DID, value
        Approval(Vec<u8>, IdentityId, IdentityId, Balance),
        /// emit when tokens get issued
        /// ticker, beneficiary DID, value
        Issued(Vec<u8>, IdentityId, Balance),
        /// emit when tokens get redeemed
        /// ticker, DID, value
        Redeemed(Vec<u8>, IdentityId, Balance),
        /// event for forced transfer of tokens
        /// ticker, controller DID, from DID, to DID, value, data, operator data
        ControllerTransfer(Vec<u8>, IdentityId, IdentityId, IdentityId, Balance, Vec<u8>, Vec<u8>),
        /// event for when a forced redemption takes place
        /// ticker, controller DID, token holder DID, value, data, operator data
        ControllerRedemption(Vec<u8>, IdentityId, IdentityId, Balance, Vec<u8>, Vec<u8>),
        /// Event for creation of the asset
        /// ticker, total supply, owner DID, divisibility
        IssuedToken(Vec<u8>, Balance, IdentityId, bool),
        /// Event for change in divisibility
        /// ticker, divisibility
        DivisibilityChanged(Vec<u8>, bool),
        /// can_transfer() output
        /// ticker, from_did, to_did, value, data, ERC1066 status
        /// 0 - OK
        /// 1,2... - Error, meanings TBD
        CanTransfer(Vec<u8>, IdentityId, IdentityId, Balance, Vec<u8>, u32),
        /// An additional event to Transfer; emitted when transfer_with_data is called; similar to
        /// Transfer with data added at the end.
        /// ticker, from DID, to DID, value, data
        TransferWithData(Vec<u8>, IdentityId, IdentityId, Balance, Vec<u8>),
        /// is_issuable() output
        /// ticker, return value (true if issuable)
        IsIssuable(Vec<u8>, bool),
        /// get_document() output
        /// ticker, name, uri, hash, last modification date
        GetDocument(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Moment),
        /// emit when tokens transferred by the custodian
        /// ticker, custodian did, holder/from did, to did, amount
        CustodyTransfer(Vec<u8>, IdentityId, IdentityId, IdentityId, Balance),
        /// emit when allowance get increased
        /// ticker, holder did, custodian did, oldAllowance, newAllowance
        CustodyAllowanceChanged(Vec<u8>, IdentityId, IdentityId, Balance, Balance),
        /// emit when ticker is registered
        /// ticker, ticker owner, expiry
        TickerRegistered(Vec<u8>, IdentityId, Option<Moment>),
        /// emit when ticker is transferred
        /// ticker, from, to
        TickerTransferred(Vec<u8>, IdentityId, IdentityId),
        /// emit when ticker is registered
        /// ticker, current owner, approved owner
        TickerTransferApproval(Vec<u8>, IdentityId, IdentityId),
        /// ticker transfer approval withdrawal
        /// ticker, approved did
        TickerTransferApprovalWithdrawal(Vec<u8>, IdentityId),
    }
}

pub trait AssetTrait<V> {
    fn total_supply(ticker: &[u8]) -> V;
    fn balance(ticker: &[u8], did: IdentityId) -> V;
    fn _mint_from_sto(ticker: &[u8], sender_did: IdentityId, tokens_purchased: V) -> Result;
    fn is_owner(ticker: &Vec<u8>, did: IdentityId) -> bool;
    fn get_balance_at(ticker: &Vec<u8>, did: IdentityId, at: u64) -> V;
}

impl<T: Trait> AssetTrait<T::Balance> for Module<T> {
    fn _mint_from_sto(ticker: &[u8], sender: IdentityId, tokens_purchased: T::Balance) -> Result {
        let upper_ticker = utils::bytes_to_upper(ticker);
        Self::_mint(&upper_ticker, sender, tokens_purchased)
    }

    fn is_owner(ticker: &Vec<u8>, did: IdentityId) -> bool {
        Self::_is_owner(ticker, did)
    }

    /// Get the asset `id` balance of `who`.
    fn balance(ticker: &[u8], who: IdentityId) -> T::Balance {
        let upper_ticker = utils::bytes_to_upper(ticker);
        return Self::balance_of((upper_ticker, who));
    }

    // Get the total supply of an asset `id`
    fn total_supply(ticker: &[u8]) -> T::Balance {
        let upper_ticker = utils::bytes_to_upper(ticker);
        return Self::token_details(upper_ticker).total_supply;
    }

    fn get_balance_at(ticker: &Vec<u8>, did: IdentityId, at: u64) -> T::Balance {
        let upper_ticker = utils::bytes_to_upper(ticker);
        return Self::get_balance_at(&upper_ticker, did, at);
    }
}

pub trait AcceptTickerTransfer {
    fn accept_ticker_transfer(to_did: IdentityId, auth_id: u64) -> Result;
}

impl<T: Trait> AcceptTickerTransfer for Module<T> {
    fn accept_ticker_transfer(to_did: IdentityId, auth_id: u64) -> Result {
        Self::_accept_ticker_transfer(to_did, auth_id)
    }
}

/// All functions in the decl_module macro become part of the public interface of the module
/// If they are there, they are accessible via extrinsics calls whether they are public or not
/// However, in the impl module section (this, below) the functions can be public and private
/// Private functions are internal to this module e.g.: _transfer
/// Public functions can be called from other modules e.g.: lock and unlock (being called from the tcr module)
/// All functions in the impl module section are not part of public interface because they are not part of the Call enum
impl<T: Trait> Module<T> {
    // Public immutables
    pub fn _is_owner(ticker: &Vec<u8>, did: IdentityId) -> bool {
        let token = Self::token_details(ticker);
        token.owner_did == did
    }

    pub fn is_ticker_available(ticker: &Vec<u8>) -> bool {
        // Assumes uppercase ticker
        if <Tickers<T>>::exists(ticker.clone()) {
            let now = <timestamp::Module<T>>::get();
            if let Some(expiry) = Self::ticker_registration(ticker.clone()).expiry {
                if now <= expiry {
                    return false;
                }
            } else {
                return false;
            }
        }
        return true;
    }

    pub fn is_ticker_registry_valid(ticker: &Vec<u8>, did: IdentityId) -> bool {
        // Assumes uppercase ticker
        if <Tickers<T>>::exists(ticker.clone()) {
            let now = <timestamp::Module<T>>::get();
            let ticker_reg = Self::ticker_registration(ticker.clone());
            if ticker_reg.owner == did {
                if let Some(expiry) = ticker_reg.expiry {
                    if now > expiry {
                        return false;
                    }
                } else {
                    return true;
                }
                return true;
            }
        }
        return false;
    }

    /// Returns 0 if ticker is registered to someone else
    /// 1 if ticker is available for registry
    /// 2 if ticker is already registered to provided did
    pub fn is_ticker_available_or_registered_to(
        ticker: &Vec<u8>,
        did: IdentityId,
    ) -> TickerRegistrationStatus {
        // Assumes uppercase ticker
        if <Tickers<T>>::exists(ticker.clone()) {
            let ticker_reg = Self::ticker_registration(ticker.clone());
            if let Some(expiry) = ticker_reg.expiry {
                let now = <timestamp::Module<T>>::get();
                if now > expiry {
                    // ticker registered to someone but expired and can be registered again
                    return TickerRegistrationStatus::Available;
                } else if ticker_reg.owner == did {
                    // ticker is already registered to provided did (but may expire in future)
                    return TickerRegistrationStatus::RegisteredByDid;
                }
            } else if ticker_reg.owner == did {
                // ticker is already registered to provided did (and will never expire)
                return TickerRegistrationStatus::RegisteredByDid;
            }
            // ticker registered to someone else
            return TickerRegistrationStatus::RegisteredByOther;
        }
        // Ticker not registered yet
        return TickerRegistrationStatus::Available;
    }

    fn _register_ticker(
        ticker: &Vec<u8>,
        sender: T::AccountId,
        to_did: IdentityId,
        expiry: Option<T::Moment>,
    ) {
        // charge fee
        Self::charge_ticker_registration_fee(ticker, sender.clone(), to_did);

        let ticker_registration = TickerRegistration {
            owner: to_did,
            expiry: expiry.clone(),
        };

        // Store ticker registration details
        <Tickers<T>>::insert(ticker, ticker_registration);

        Self::deposit_event(RawEvent::TickerRegistered(ticker.to_vec(), to_did, expiry));
    }

    fn charge_ticker_registration_fee(_ticker: &Vec<u8>, _sender: T::AccountId, _did: IdentityId) {
        //TODO: Charge fee
    }

    /// Get the asset `id` balance of `who`.
    pub fn balance(ticker: &Vec<u8>, did: IdentityId) -> T::Balance {
        let upper_ticker = utils::bytes_to_upper(ticker);
        Self::balance_of((upper_ticker, did))
    }

    // Get the total supply of an asset `id`
    pub fn total_supply(ticker: &[u8]) -> T::Balance {
        let upper_ticker = utils::bytes_to_upper(ticker);
        Self::token_details(upper_ticker).total_supply
    }

    pub fn get_balance_at(ticker: &Vec<u8>, did: IdentityId, at: u64) -> T::Balance {
        let upper_ticker = utils::bytes_to_upper(ticker);
        let ticker_did = (upper_ticker.clone(), did);
        if !<TotalCheckpoints>::exists(upper_ticker.clone()) ||
            at == 0 || //checkpoints start from 1
            at > Self::total_checkpoints_of(&upper_ticker)
        {
            // No checkpoints data exist
            return Self::balance_of(&ticker_did);
        }

        if <UserCheckpoints>::exists(&ticker_did) {
            let user_checkpoints = Self::user_checkpoints(&ticker_did);
            if at > *user_checkpoints.last().unwrap_or(&0) {
                // Using unwrap_or to be defensive.
                // or part should never be triggered due to the check on 2 lines above
                // User has not transacted after checkpoint creation.
                // This means their current balance = their balance at that cp.
                return Self::balance_of(&ticker_did);
            }
            // Uses the first checkpoint that was created after target checpoint
            // and the user has data for that checkpoint
            return Self::balance_at_checkpoint((
                upper_ticker.clone(),
                did,
                Self::find_ceiling(&user_checkpoints, at),
            ));
        }
        // User has no checkpoint data.
        // This means that user's balance has not changed since first checkpoint was created.
        // Maybe the user never held any balance.
        return Self::balance_of(&ticker_did);
    }

    fn find_ceiling(arr: &Vec<u64>, key: u64) -> u64 {
        // This function assumes that key <= last element of the array,
        // the array consists of unique sorted elements,
        // array len > 0
        let mut end = arr.len();
        let mut start = 0;
        let mut mid = (start + end) / 2;

        while mid != 0 && end >= start {
            // Due to our assumptions, we can even remove end >= start condition from here
            if key > arr[mid - 1] && key <= arr[mid] {
                // This condition and the fact that key <= last element of the array mean that
                // start should never become greater than end.
                return arr[mid];
            } else if key > arr[mid] {
                start = mid + 1;
            } else {
                end = mid;
            }
            mid = (start + end) / 2;
        }

        // This should only be reached when mid becomes 0.
        return arr[0];
    }

    fn _is_valid_transfer(
        ticker: &Vec<u8>,
        from_did: Option<IdentityId>,
        to_did: Option<IdentityId>,
        value: T::Balance,
    ) -> StdResult<u8, &'static str> {
        let general_status_code =
            <general_tm::Module<T>>::verify_restriction(ticker, from_did, to_did, value)?;
        Ok(if general_status_code != ERC1400_TRANSFER_SUCCESS {
            general_status_code
        } else {
            <percentage_tm::Module<T>>::verify_restriction(ticker, from_did, to_did, value)?
        })
    }

    // the SimpleToken standard transfer function
    // internal
    fn _transfer(
        ticker: &Vec<u8>,
        from_did: IdentityId,
        to_did: IdentityId,
        value: T::Balance,
    ) -> Result {
        // Granularity check
        ensure!(
            Self::check_granularity(ticker, value),
            "Invalid granularity"
        );
        let ticker_from_did = (ticker.clone(), from_did);
        ensure!(
            <BalanceOf<T>>::exists(&ticker_from_did),
            "Account does not own this token"
        );
        let sender_balance = Self::balance_of(&ticker_from_did);
        ensure!(sender_balance >= value, "Not enough balance.");

        let updated_from_balance = sender_balance
            .checked_sub(&value)
            .ok_or("overflow in calculating balance")?;
        let ticker_to_did = (ticker.clone(), to_did);
        let receiver_balance = Self::balance_of(&ticker_to_did);
        let updated_to_balance = receiver_balance
            .checked_add(&value)
            .ok_or("overflow in calculating balance")?;

        Self::_update_checkpoint(ticker, from_did, sender_balance);
        Self::_update_checkpoint(ticker, to_did, receiver_balance);
        // reduce sender's balance
        <BalanceOf<T>>::insert(ticker_from_did, updated_from_balance);

        // increase receiver's balance
        <BalanceOf<T>>::insert(ticker_to_did, updated_to_balance);

        // Update statistic info.
        <statistics::Module<T>>::update_transfer_stats(
            ticker,
            Some(updated_from_balance),
            Some(updated_to_balance),
            value,
        );

        Self::deposit_event(RawEvent::Transfer(ticker.clone(), from_did, to_did, value));
        Ok(())
    }

    pub fn _create_checkpoint(ticker: &Vec<u8>) -> Result {
        if <TotalCheckpoints>::exists(ticker) {
            let mut checkpoint_count = Self::total_checkpoints_of(ticker);
            checkpoint_count = checkpoint_count
                .checked_add(1)
                .ok_or("overflow in adding checkpoint")?;
            <TotalCheckpoints>::insert(ticker, checkpoint_count);
            <CheckpointTotalSupply<T>>::insert(
                (ticker.clone(), checkpoint_count),
                Self::token_details(ticker).total_supply,
            );
        } else {
            <TotalCheckpoints>::insert(ticker, 1);
            <CheckpointTotalSupply<T>>::insert(
                (ticker.clone(), 1),
                Self::token_details(ticker).total_supply,
            );
        }
        Ok(())
    }

    fn _update_checkpoint(ticker: &Vec<u8>, user_did: IdentityId, user_balance: T::Balance) {
        if <TotalCheckpoints>::exists(ticker) {
            let checkpoint_count = Self::total_checkpoints_of(ticker);
            let ticker_user_did_checkpont = (ticker.clone(), user_did, checkpoint_count);
            if !<CheckpointBalance<T>>::exists(&ticker_user_did_checkpont) {
                <CheckpointBalance<T>>::insert(&ticker_user_did_checkpont, user_balance);
                <UserCheckpoints>::mutate((ticker.clone(), user_did), |user_checkpoints| {
                    user_checkpoints.push(checkpoint_count);
                });
            }
        }
    }

    fn is_owner(ticker: &Vec<u8>, did: IdentityId) -> bool {
        Self::_is_owner(ticker, did)
    }

    pub fn _mint(ticker: &Vec<u8>, to_did: IdentityId, value: T::Balance) -> Result {
        // Granularity check
        ensure!(
            Self::check_granularity(ticker, value),
            "Invalid granularity"
        );
        //Increase receiver balance
        let ticker_to_did = (ticker.clone(), to_did);
        let current_to_balance = Self::balance_of(&ticker_to_did);
        let updated_to_balance = current_to_balance
            .checked_add(&value)
            .ok_or("overflow in calculating balance")?;
        // verify transfer check
        ensure!(
            Self::_is_valid_transfer(ticker, None, Some(to_did), value)?
                == ERC1400_TRANSFER_SUCCESS,
            "Transfer restrictions failed"
        );

        // Read the token details
        let mut token = Self::token_details(ticker);
        let updated_total_supply = token
            .total_supply
            .checked_add(&value)
            .ok_or("overflow in calculating total supply")?;
        ensure!(
            updated_total_supply <= MAX_SUPPLY.into(),
            "Total supply above the limit"
        );
        //Increase total suply
        token.total_supply = updated_total_supply;

        Self::_update_checkpoint(ticker, to_did, current_to_balance);

        <BalanceOf<T>>::insert(&ticker_to_did, updated_to_balance);
        <Tokens<T>>::insert(ticker, token);
        <statistics::Module<T>>::update_transfer_stats(
            &ticker,
            None,
            Some(updated_to_balance),
            value,
        );

        Self::deposit_event(RawEvent::Issued(ticker.clone(), to_did, value));

        Ok(())
    }

    fn check_granularity(ticker: &Vec<u8>, value: T::Balance) -> bool {
        // Read the token details
        let token = Self::token_details(ticker);
        token.divisible || value % ONE_UNIT.into() == 0.into()
    }

    fn _check_custody_allowance(
        ticker: &Vec<u8>,
        holder_did: IdentityId,
        value: T::Balance,
    ) -> Result {
        let remaining_balance = Self::balance_of((ticker.clone(), holder_did))
            .checked_sub(&value)
            .ok_or("underflow in balance deduction")?;
        ensure!(
            remaining_balance >= Self::total_custody_allowance((ticker.clone(), holder_did)),
            "Insufficient balance for transfer"
        );
        Ok(())
    }

    fn _increase_custody_allowance(
        ticker: Vec<u8>,
        holder_did: IdentityId,
        custodian_did: IdentityId,
        value: T::Balance,
    ) -> Result {
        let new_custody_allowance = Self::total_custody_allowance((ticker.clone(), holder_did))
            .checked_add(&value)
            .ok_or("total custody allowance get overflowed")?;
        // Ensure that balance of the token holder should greater than or equal to the total custody allowance + value
        ensure!(
            Self::balance_of((ticker.clone(), holder_did)) >= new_custody_allowance,
            "Insufficient balance of holder did"
        );
        // Ensure the valid DID
        ensure!(
            <identity::DidRecords>::exists(custodian_did),
            "Invalid custodian DID"
        );

        let old_allowance = Self::custodian_allowance((ticker.clone(), holder_did, custodian_did));
        let new_current_allowance = old_allowance
            .checked_add(&value)
            .ok_or("allowance get overflowed")?;
        // Update Storage
        <CustodianAllowance<T>>::insert(
            (ticker.clone(), holder_did, custodian_did),
            &new_current_allowance,
        );
        <TotalCustodyAllowance<T>>::insert((ticker.clone(), holder_did), new_custody_allowance);
        Self::deposit_event(RawEvent::CustodyAllowanceChanged(
            ticker.clone(),
            holder_did,
            custodian_did,
            old_allowance,
            new_current_allowance,
        ));
        Ok(())
    }

    pub fn _accept_ticker_transfer(to_did: IdentityId, auth_id: u64) -> Result {
        ensure!(
            <identity::Authorizations<T>>::exists((Signer::from(to_did), auth_id)),
            AuthorizationError::Invalid.into()
        );

        let auth = <identity::Module<T>>::authorizations((Signer::from(to_did), auth_id));

        let ticker = match auth.authorization_data {
            AuthorizationData::TransferTicker(_ticker) => utils::bytes_to_upper(_ticker.as_slice()),
            _ => return Err("Not a ticker transfer auth"),
        };

        ensure!(!<Tokens<T>>::exists(&ticker), "token already created");

        let current_owner = Self::ticker_registration(&ticker).owner;

        <identity::Module<T>>::consume_auth(
            Signer::from(current_owner),
            Signer::from(to_did),
            auth_id,
        )?;

        <Tickers<T>>::mutate(&ticker, |tr| tr.owner = to_did);

        Self::deposit_event(RawEvent::TickerTransferred(ticker, current_owner, to_did));

        Ok(())
    }
}
