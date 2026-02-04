use algebra::random::DiscreteGaussian;
use mpc::MPCBackend;
use rand::{prelude::Distribution, Rng};
use std::fmt::Debug;
use tracing::{info, instrument};

#[derive(Debug, Clone, Default)]
pub struct MPCLwe<Share: Default> {
    pub a: Vec<u64>,
    pub b: Share,
}

impl<Share: Default> MPCLwe<Share> {
    pub fn zero(dimension: usize) -> Self {
        MPCLwe {
            a: vec![0; dimension],
            b: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatchMPCLwe<Share: Default> {
    pub a: Vec<Vec<u64>>,
    pub b: Vec<Share>,
}

#[instrument(skip_all)]
pub async fn generate_shared_lwe_ciphertext_vec<Backend, R>(
    backend: &mut Backend,
    shared_secret_key: &[Backend::Sharing],
    count: usize,
    gaussian: &DiscreteGaussian<u64>,
    rng: &mut R,
) -> BatchMPCLwe<Backend::Sharing>
where
    Backend: MPCBackend,
    R: Rng,
{
    info!(id = backend.party_id(), "Generating shared LWE ciphertext");
    let mut batch_mpc_lwe = BatchMPCLwe {
        a: vec![vec![0; shared_secret_key.len()]; count],
        b: vec![Default::default(); count],
    };

    batch_mpc_lwe.a.iter_mut().for_each(|a| {
        backend.shared_rand_field_elements(a);
    });

    let b = &mut batch_mpc_lwe.b;
    let e_will_share = gaussian
        .sample_iter(&mut *rng)
        .take(count)
        .collect::<Vec<_>>();

    backend
        .all_paries_sends_slice_to_all_parties_sum(&e_will_share, count, b)
        .await;

    batch_mpc_lwe
        .a
        .iter()
        .zip(batch_mpc_lwe.b.iter_mut())
        .for_each(|(a, b)| {
            let ip = backend.inner_product_const(shared_secret_key, a);
            *b = backend.add(ip, *b);
        });

    batch_mpc_lwe
}

pub async fn generate_shared_lwe_ciphertext<Backend, R>(
    backend: &mut Backend,
    shared_secret_key: &[Backend::Sharing],
    gaussian: &DiscreteGaussian<u64>,
    rng: &mut R,
) -> MPCLwe<Backend::Sharing>
where
    Backend: MPCBackend,
    R: Rng,
{
    info!(id = backend.party_id(), "Generating shared LWE ciphertext");
    let id = backend.party_id();
    let mut a = vec![0; shared_secret_key.len()];
    backend.shared_rand_field_elements(&mut a);

    let e_wil_share = gaussian.sample(rng);
    let mut e_vec = Vec::new();
    for i in 0..backend.num_parties() {
        if i == id {
            e_vec.push(backend.input(Some(e_wil_share), i).await.unwrap());
        } else {
            e_vec.push(backend.input(None, i).await.unwrap())
        }
    }

    let e = e_vec.into_iter().reduce(|x, y| backend.add(x, y)).unwrap();

    let b = backend.inner_product_const(shared_secret_key, &a);
    let b = backend.add(b, e);

    MPCLwe { a, b }
}
