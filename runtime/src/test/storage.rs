use crate::{
    asset::{self, TickerRegistrationConfig},
    balances, exemption, general_tm, identity, percentage_tm, statistics, utils,
};
use primitives::{IdentityId, Key};

use codec::Encode;
use sr_io::TestExternalities;
use sr_primitives::{
    testing::{Header, UintAuthorityId},
    traits::{BlakeTwo256, ConvertInto, IdentityLookup, OpaqueKeys, Verify},
    AnySignature, Perbill,
};
use srml_support::{
    dispatch::{DispatchError, DispatchResult},
    impl_outer_origin, parameter_types,
    traits::Currency,
};
use std::convert::TryFrom;
use substrate_primitives::{crypto::Pair as PairTrait, sr25519::Pair, Blake2Hasher, H256};
use test_client::AccountKeyring;

impl_outer_origin! {
    pub enum Origin for TestStorage {}
}

// For testing the module, we construct most of a mock runtime. This means
// first constructing a configuration type (`Test`) which `impl`s each of the
// configuration traits of modules we want to use.
#[derive(Clone, Eq, PartialEq)]
pub struct TestStorage;

type AccountId = <AnySignature as Verify>::Signer;
type Index = u64;
type BlockNumber = u64;
type Hash = H256;
type Hashing = BlakeTwo256;
type Lookup = IdentityLookup<AccountId>;
type OffChainSignature = AnySignature;
type SessionIndex = u32;
type AuthorityId = <AnySignature as Verify>::Signer;
type WeightMultiplierUpdate = ();
type Event = ();
type Version = ();

parameter_types! {
    pub const BlockHashCount: u32 = 250;
    pub const MaximumBlockWeight: u32 = 4096;
    pub const MaximumBlockLength: u32 = 4096;
    pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
}

impl system::Trait for TestStorage {
    type Origin = Origin;
    type Index = Index;
    type BlockNumber = BlockNumber;
    type Hash = Hash;
    type Hashing = Hashing;
    type AccountId = AccountId;
    type Lookup = Lookup;
    type Header = Header;
    type Event = Event;

    type Call = ();
    type WeightMultiplierUpdate = WeightMultiplierUpdate;
    type BlockHashCount = BlockHashCount;
    type MaximumBlockWeight = MaximumBlockWeight;
    type MaximumBlockLength = MaximumBlockLength;
    type AvailableBlockRatio = AvailableBlockRatio;
    type Version = Version;
}

parameter_types! {
    pub const ExistentialDeposit: u64 = 0;
    pub const TransferFee: u64 = 0;
    pub const CreationFee: u64 = 0;
    pub const TransactionBaseFee: u64 = 0;
    pub const TransactionByteFee: u64 = 0;
}

impl balances::Trait for TestStorage {
    type Balance = u128;
    type OnFreeBalanceZero = ();
    type OnNewAccount = ();
    type Event = Event;
    type TransactionPayment = ();
    type DustRemoval = ();
    type TransferPayment = ();

    type ExistentialDeposit = ExistentialDeposit;
    type TransferFee = TransferFee;
    type CreationFee = CreationFee;
    type TransactionBaseFee = TransactionBaseFee;
    type TransactionByteFee = TransactionByteFee;
    type WeightToFee = ConvertInto;
    type Identity = crate::identity::Module<TestStorage>;
}

parameter_types! {
    pub const MinimumPeriod: u64 = 3;
}

impl timestamp::Trait for TestStorage {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
}

#[derive(codec::Encode, codec::Decode, Debug, Clone, Eq, PartialEq)]
pub struct IdentityProposal {
    pub dummy: u8,
}

impl sr_primitives::traits::Dispatchable for IdentityProposal {
    type Origin = Origin;
    type Trait = TestStorage;
    type Error = DispatchError;

    fn dispatch(self, _origin: Self::Origin) -> DispatchResult<Self::Error> {
        Ok(())
    }
}

impl identity::Trait for TestStorage {
    type Event = Event;
    type Proposal = IdentityProposal;
    type AcceptTickerTransferTarget = TestStorage;
}

impl crate::asset::AcceptTickerTransfer for TestStorage {
    fn accept_ticker_transfer(_: IdentityId, _: u64) -> Result<(), &'static str> {
        Ok(())
    }
}

impl statistics::Trait for TestStorage {}

impl percentage_tm::Trait for TestStorage {
    type Event = Event;
}

impl general_tm::Trait for TestStorage {
    type Event = Event;
    type Asset = asset::Module<TestStorage>;
}

impl asset::Trait for TestStorage {
    type Event = Event;
    type Currency = balances::Module<TestStorage>;
}

impl exemption::Trait for TestStorage {
    type Event = Event;
    type Asset = asset::Module<TestStorage>;
}

impl utils::Trait for TestStorage {
    type OffChainSignature = OffChainSignature;
    fn validator_id_to_account_id(v: <Self as session::Trait>::ValidatorId) -> Self::AccountId {
        v
    }
}

pub struct TestOnSessionEnding;
impl session::OnSessionEnding<AuthorityId> for TestOnSessionEnding {
    fn on_session_ending(_: SessionIndex, _: SessionIndex) -> Option<Vec<AuthorityId>> {
        None
    }
}

pub struct TestSessionHandler;
impl session::SessionHandler<AuthorityId> for TestSessionHandler {
    fn on_new_session<Ks: OpaqueKeys>(
        _changed: bool,
        _validators: &[(AuthorityId, Ks)],
        _queued_validators: &[(AuthorityId, Ks)],
    ) {
    }

    fn on_disabled(_validator_index: usize) {}

    fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(AuthorityId, Ks)]) {}
}

parameter_types! {
    pub const Period: BlockNumber = 1;
    pub const Offset: BlockNumber = 0;
    pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
}

impl session::Trait for TestStorage {
    type OnSessionEnding = TestOnSessionEnding;
    type Keys = UintAuthorityId;
    type ShouldEndSession = session::PeriodicSessions<Period, Offset>;
    type SessionHandler = TestSessionHandler;
    type Event = Event;
    type ValidatorId = AuthorityId;
    type ValidatorIdOf = ConvertInto;
    type SelectInitialValidators = ();
    type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
}

// Publish type alias for each module
pub type Identity = identity::Module<TestStorage>;
pub type Balances = balances::Module<TestStorage>;
pub type Asset = asset::Module<TestStorage>;

/// Create externalities
pub fn build_ext() -> TestExternalities<Blake2Hasher> {
    let mut storage = system::GenesisConfig::default()
        .build_storage::<TestStorage>()
        .unwrap();

    // Identity genesis.
    identity::GenesisConfig::<TestStorage> {
        owner: AccountKeyring::Alice.public().into(),
        did_creation_fee: 250,
    }
    .assimilate_storage(&mut storage)
    .unwrap();

    // Asset genesis.
    asset::GenesisConfig::<TestStorage> {
        asset_creation_fee: 0,
        ticker_registration_fee: 0,
        ticker_registration_config: TickerRegistrationConfig {
            max_ticker_length: 12,
            registration_length: Some(10000),
        },
        fee_collector: AccountKeyring::Dave.public().into(),
    }
    .assimilate_storage(&mut storage)
    .unwrap();

    sr_io::TestExternalities::new(storage)
}

pub fn make_account(
    id: AccountId,
) -> Result<(<TestStorage as system::Trait>::Origin, IdentityId), &'static str> {
    make_account_with_balance(id, 1_000)
}

/// It creates an Account and registers its DID.
pub fn make_account_with_balance(
    id: AccountId,
    balance: <TestStorage as balances::Trait>::Balance,
) -> Result<(<TestStorage as system::Trait>::Origin, IdentityId), &'static str> {
    let signed_id = Origin::signed(id.clone());
    Balances::make_free_balance_be(&id, balance);

    Identity::register_did(signed_id.clone(), vec![])?;
    let did = Identity::get_identity(&Key::try_from(id.encode())?).unwrap();

    Ok((signed_id, did))
}

pub fn register_keyring_account(acc: AccountKeyring) -> Result<IdentityId, &'static str> {
    register_keyring_account_with_balance(acc, 10_000)
}

pub fn register_keyring_account_with_balance(
    acc: AccountKeyring,
    balance: <TestStorage as balances::Trait>::Balance,
) -> Result<IdentityId, &'static str> {
    Balances::make_free_balance_be(&acc.public(), balance);

    let acc_pub = acc.public();
    Identity::register_did(Origin::signed(acc_pub.clone()), vec![])?;

    let acc_key = Key::from(acc_pub.0);
    let did =
        Identity::get_identity(&acc_key).ok_or_else(|| "Key cannot be generated from account")?;

    Ok(did)
}

pub fn account_from(id: u64) -> AccountId {
    let mut enc_id_vec = id.encode();
    enc_id_vec.resize_with(32, Default::default);

    let mut enc_id = [0u8; 32];
    enc_id.copy_from_slice(enc_id_vec.as_slice());

    Pair::from_seed(&enc_id).public()
}
