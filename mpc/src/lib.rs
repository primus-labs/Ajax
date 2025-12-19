#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![deny(missing_docs)]
//! This crate provides backend for various MPC operations over a network.
use std::time::Duration;
pub mod dn;
pub mod error;

use algebra::reduce::FieldReduce;
pub use dn::DNBackend;
use std::fmt::Debug;

type MPCResult<T> = Result<T, error::MPCErr>;

/// MPC backend trait
#[allow(async_fn_in_trait)]
pub trait MPCBackend {
    /// Generic secret sharing type.
    type Sharing: Clone + Copy + Default + Debug + Send;

    /// Generic field modulus type.
    type Modulus: FieldReduce<u64>;

    /// Get the party id.
    fn party_id(&self) -> usize;

    /// Get the number of parties.
    fn num_parties(&self) -> usize;

    /// Get the number of threshold.
    fn num_threshold(&self) -> usize;

    /// Get the field modulus.
    fn modulus(&self) -> Self::Modulus;

    /// Get the field modulus.
    fn field_modulus_value(&self) -> u64;

    /// Negate a secret share.
    fn neg(&self, a: Self::Sharing) -> Self::Sharing;

    /// Add two secret shares.
    fn add(&self, a: Self::Sharing, b: Self::Sharing) -> Self::Sharing;

    /// Double a secret share.
    fn double(&self, a: Self::Sharing) -> Self::Sharing;

    /// Add two secret shares.
    fn add_const(&self, a: Self::Sharing, b: u64) -> Self::Sharing;

    /// Add two secret shares.
    fn add_const_pub(a: Self::Sharing, b: u64) -> Self::Sharing;

    /// Subtract two secret shares.
    fn sub(&self, a: Self::Sharing, b: Self::Sharing) -> Self::Sharing;

    /// Multiply a secret share with a constant.
    fn mul_const(&self, a: Self::Sharing, b: u64) -> Self::Sharing;

    /// Multiply two secret shares locally.
    fn mul_local(&self, a: Self::Sharing, b: Self::Sharing) -> Self::Sharing;

    /// Multiply two secret shares.
    async fn mul(&mut self, a: Self::Sharing, b: Self::Sharing) -> MPCResult<Self::Sharing>;

    /// Multiply batch of secret shares.
    async fn mul_element_wise(
        &mut self,
        a: &[Self::Sharing],
        b: &[Self::Sharing],
    ) -> MPCResult<Vec<Self::Sharing>>;

    /// Multiply batch of secret shares use double random.
    async fn double_mul_element_wise(
        &mut self,
        a: &[Self::Sharing],
        b: &[Self::Sharing],
    ) -> MPCResult<Vec<Self::Sharing>>;

    /// Inner product of two arrays of secret shares.
    async fn inner_product(
        &mut self,
        a: &[Self::Sharing],
        b: &[Self::Sharing],
    ) -> MPCResult<Self::Sharing>;

    /// Inner product of an array of secret shares with an array of constants.
    fn inner_product_const(&mut self, a: &[Self::Sharing], b: &[u64]) -> Self::Sharing;

    /// Input a secret value from a party (party_id). Inputs from all other parties are omitted.
    async fn input(&mut self, value: Option<u64>, party_id: usize) -> MPCResult<Self::Sharing>;

    /// Input several secret values from a party (party_id). Inputs from all other parties are omitted.
    async fn input_slice(
        &self,
        values: Option<&[u64]>,
        batch_size: usize,
        party_id: usize,
    ) -> MPCResult<Vec<Self::Sharing>>;

    /// Input several secret values from different parties.
    async fn input_slice_with_different_party_ids(
        &mut self,
        values: &[Option<u64>],
        party_ids: &[usize],
    ) -> MPCResult<Vec<Self::Sharing>>;

    /// Output a secret value to a party (party_id). Other parties get a dummy value.
    async fn reveal(&mut self, share: Self::Sharing, party_id: usize) -> MPCResult<Option<u64>>;

    /// Output a slice of secret values to a party (party_id). Other parties get dummy values.
    async fn reveal_slice(
        &mut self,
        shares: &[Self::Sharing],
        party_id: usize,
    ) -> MPCResult<Vec<Option<u64>>>;

    /// Output a secret value to all parties.
    async fn reveal_to_all(&mut self, share: Self::Sharing) -> MPCResult<u64>;

    /// Output a slice of secret values to all parties.
    async fn reveal_slice_to_all(&mut self, shares: &[Self::Sharing]) -> MPCResult<Vec<u64>>;

    /// Reveal a slice of secret values to all parties.
    async fn reveal_slice_degree_2t_to_all(
        &mut self,
        shares: &[Self::Sharing],
    ) -> MPCResult<Vec<u64>>;

    /// Generate a random value over `u64`.
    fn shared_rand_coin(&mut self) -> u64;

    /// Generate a random value over a specific field.
    fn shared_rand_field_element(&mut self) -> u64;

    /// Generate random values over a specific field.
    fn shared_rand_field_elements(&mut self, destination: &mut [u64]);

    /// Generates a batch of random elements.
    async fn create_random_elements(&mut self, batch_size: usize) -> Vec<Self::Sharing>;

    /// Transform a polynomial to NTT domain.
    fn ntt_sharing_poly_inplace(&self, poly: &mut [Self::Sharing]);

    /// Transform a polynomial to NTT domain.
    fn ntt_poly_inplace(&self, poly: &mut [u64]);

    ///  multipliaction for shares over z2k
    async fn mul_element_wise_z2k(&mut self, a: &[u64], b: &[u64], k: u32) -> Vec<u64>;

    /// init z2k triples, read triples from files
    fn init_z2k_triples_from_files(&mut self);

    /// Output a slice of secret values over z2k to all parties.
    async fn reveal_slice_to_all_z2k(
        &mut self,
        shares: &[u64],
        k: u32,
        need_leader: bool,
    ) -> Vec<u64>;

    /// test
    async fn test_open_secrets_z2k(
        &mut self,
        reconstructor_id: usize,
        degree: usize,
        shares: &[u64],
        broadcast_result: bool,
    ) -> Option<Vec<u64>>;

    /// reveal_slice_z2k
    async fn reveal_slice_z2k(
        &mut self,
        shares: &[u64],
        party_id: usize,
        k: u32,
    ) -> Vec<Option<u64>>;

    /// input slice over z2k
    async fn input_slice_z2k(
        &mut self,
        values: Option<&[u64]>,
        batch_size: usize,
        party_id: usize,
    ) -> Vec<u64>;

    /// add vec additive secret sharing over z2k
    fn add_z2k_slice(&self, a: &[u64], b: &[u64], k: u32) -> Vec<u64>;

    /// sub vec additive secret sharing over z2k
    fn sub_z2k_slice(&self, a: &[u64], b: &[u64], k: u32) -> Vec<u64>;

    /// double vec additive secret sharing over z2k
    fn double_z2k_slice(&self, a: &[u64], k: u32) -> Vec<u64>;

    /// convert additive secret sharing to additive secret sharing
    fn shamir_secrets_to_additive_secrets(&mut self, shares: &[Self::Sharing]) -> Vec<u64>;

    /// addition between a consant a and an additive secret sharing b
    fn add_z2k_const(&mut self, a: u64, b: u64, k: u32) -> u64;

    /// return additive secret sharing of a-b where a is const and b is additive sharing
    fn sub_z2k_const(&mut self, a: u64, b: u64, k: u32) -> u64;

    /// sub between a consant a and an additive secret sharing b over F_p
    fn sub_additive_const_p(&mut self, a: u64, b: u64) -> u64;

    /// mul between a consant a and an additive secret sharing b over F_p
    fn mul_additive_const_p(&mut self, a: u64, b: u64) -> u64;

    ///inner product between a consant vec a and an additive secret sharing vec b over F_p
    fn inner_product_additive_const_p(&mut self, a: &[u64], b: &[u64]) -> u64;

    /// sends public message to all parties
    async fn sends_slice_to_all_parties(
        &mut self,
        values: Option<&[u64]>,
        batch_size: usize,
        party_id: usize,
    ) -> Vec<u64>;

    /// input slice over z2k and share with prg
    fn input_slice_with_prg_z2k(
        &mut self,
        values: Option<&[u64]>,
        batch_size: usize,
        party_id: usize,
    ) -> Vec<u64>;

    /// input slice by shamir and share with prg
    async fn input_slice_with_prg(
        &self,
        values: Option<&[u64]>,
        batch_size: usize,
        party_id: usize,
        degree: usize,
    ) -> MPCResult<Vec<Self::Sharing>>;

    /// all parties sends slice to all parties, and sum them up with sum_result, e.g. sum_result = sum_result + \sum_{i=0}^{n-1} values
    async fn all_paries_sends_slice_to_all_parties_sum(
        &self,
        values: &[u64],
        batch_size: usize,
        sum_result: &mut [Self::Sharing],
    );

    /// count double random times
    fn total_mul_triple_duration(&mut self) -> Duration;

    /// return additive secret sharing of a-b where a is additive sharing and b is const
    fn sub_z2k_const_a_sub_c(&mut self, a: u64, b: u64, k: u32) -> u64;

    /// all parties sends slice to all parties, and sum them up with sum_result, e.g. sum_result = sum_result + \sum_{i=0}^{n-1} values,  with prg
    async fn all_paries_sends_slice_to_all_parties_sum_with_prg(
        &self,
        values: &[u64],
        batch_size: usize,
        sum_result: &mut [Self::Sharing],
    );

    ///print net info
    fn print_net_stats(&mut self, msg: &str);
}
