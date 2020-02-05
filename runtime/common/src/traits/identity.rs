use crate::traits::{
    balances::{self, imbalances::NegativeImbalance},
    group::GroupTrait,
    multisig::AddSignerMultiSig,
    CommonTrait,
};
use polymesh_primitives::{
    AuthorizationData, IdentityId, Key, LinkData, Permission, Signer, SigningItem,
};

use frame_support::{decl_event, weights::GetDispatchInfo, Parameter};
use frame_system;
use sp_core::H512;
use sp_runtime::traits::Dispatchable;
use sp_std::vec::Vec;

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct Claim<U> {
    pub issuance_date: U,
    pub expiry: U,
    pub claim_value: ClaimValue,
}

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct ClaimMetaData {
    pub claim_key: Vec<u8>,
    pub claim_issuer: IdentityId,
}

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct ClaimValue {
    pub data_type: DataTypes,
    pub value: Vec<u8>,
}

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
/// A structure for passing claims to `add_claims_batch`. The type argument is required to be
/// `timestamp::Trait::Moment`.
pub struct ClaimRecord<U> {
    pub did: IdentityId,
    pub claim_key: Vec<u8>,
    pub expiry: U,
    pub claim_value: ClaimValue,
}

#[derive(codec::Encode, codec::Decode, Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum DataTypes {
    U8,
    U16,
    U32,
    U64,
    U128,
    Bool,
    VecU8,
}

impl Default for DataTypes {
    fn default() -> Self {
        DataTypes::VecU8
    }
}

/// Keys could be linked to several identities (`IdentityId`) as master key or signing key.
/// Master key or external type signing key are restricted to be linked to just one identity.
/// Other types of signing key could be associated with more than one identity.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
pub enum LinkedKeyInfo {
    Unique(IdentityId),
    Group(Vec<IdentityId>),
}

pub type AuthorizationNonce = u64;

/// It represents an authorization that any account could sing to allow operations related with a
/// target identity.
///
/// # Safety
///
/// Please note, that `nonce` has been added to avoid **replay attack** and it should be the current
/// value of nonce of master key of `target_id`. See `System::account_nonce`.
/// In this way, the authorization is delimited to an specific transaction (usually the next one)
/// of master key of target identity.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
pub struct TargetIdAuthorization<Moment> {
    /// Target identity which is authorized to make an operation.
    pub target_id: IdentityId,
    /// It HAS TO be `target_id` authorization nonce: See `Identity::offchain_authorization_nonce`
    pub nonce: AuthorizationNonce,
    pub expires_at: Moment,
}

/// It is a signing item with authorization of that singning key (off-chain operation) to be added
/// to an identity.
/// `auth_signature` is the signature, generated by signing item, of `TargetIdAuthorization`.
///
/// # TODO
///  - Replace `H512` type by a template type which represents explicitly the relation with
///  `TargetIdAuthorization`.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
pub struct SigningItemWithAuth {
    /// Signing item to be added.
    pub signing_item: SigningItem,
    /// Off-chain authorization signature.
    pub auth_signature: H512,
}

/// The module's configuration trait.
pub trait Trait: CommonTrait + pallet_timestamp::Trait + balances::Trait {
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    /// An extrinsic call.
    type Proposal: Parameter + Dispatchable<Origin = Self::Origin> + GetDispatchInfo;
    /// MultiSig module
    type AddSignerMultiSigTarget: AddSignerMultiSig;
    /// Group module
    type KYCServiceProviders: GroupTrait;

    type Balances: balances::BalancesTrait<
        <Self as frame_system::Trait>::AccountId,
        <Self as CommonTrait>::Balance,
        NegativeImbalance<<Self as CommonTrait>::Balance>,
    >;
}
// rustfmt adds a commna after Option<Moment> in NewAuthorization and it breaks compilation
#[rustfmt::skip]
decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        Moment = <T as pallet_timestamp::Trait>::Moment,
    {
        /// DID, master key account ID, signing keys
        NewDid(IdentityId, AccountId, Vec<SigningItem>),

        /// DID, new keys
        NewSigningItems(IdentityId, Vec<SigningItem>),

        /// DID, the keys that got removed
        RevokedSigningItems(IdentityId, Vec<Signer>),

        /// DID, updated signing key, previous permissions
        SigningPermissionsUpdated(IdentityId, SigningItem, Vec<Permission>),

        /// DID, old master key account ID, new key
        NewMasterKey(IdentityId, AccountId, Key),

        /// DID, claim issuer DID
        NewClaimIssuer(IdentityId, IdentityId),

        /// DID, removed claim issuer DID
        RemovedClaimIssuer(IdentityId, IdentityId),

        /// DID, claim issuer DID, claims
        NewClaims(IdentityId, ClaimMetaData, Claim<Moment>),

        /// DID, claim issuer DID, claim
        RevokedClaim(IdentityId, ClaimMetaData),

        /// DID
        NewIssuer(IdentityId),

        /// DID queried
        DidQuery(Key, IdentityId),

        /// To query the status of DID
        MyKycStatus(IdentityId, bool, Option<IdentityId>),

        /// New authorization added (auth_id, from, to, authorization_data, expiry)
        NewAuthorization(u64, Signer, Signer, AuthorizationData, Option<Moment>),

        /// Authorization revoked or consumed. (auth_id, authorized_identity)
        AuthorizationRemoved(u64, Signer),

        /// New link added (link_id, associated identity or key, link_data, expiry)
        NewLink(u64, Signer, LinkData, Option<Moment>),

        /// Link removed. (link_id, associated identity or key)
        LinkRemoved(u64, Signer),

        /// Signer approved a previous request to join to a target identity.
        SignerJoinedToIdentityApproved(Signer, IdentityId),
    }
);

pub trait IdentityTrait {
    fn get_identity(key: &Key) -> Option<IdentityId>;
    fn is_signer_authorized(did: IdentityId, signer: &Signer) -> bool;
    fn is_signer_authorized_with_permissions(
        did: IdentityId,
        signer: &Signer,
        permissions: Vec<Permission>,
    ) -> bool;
    fn is_master_key(did: IdentityId, key: &Key) -> bool;
}
