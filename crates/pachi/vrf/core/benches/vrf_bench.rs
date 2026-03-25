use alloy_primitives::B256;
use criterion::{criterion_group, criterion_main, Criterion};
use pachi_vrf_core::{vrf_compute, vrf_verify};
use sha2::{Digest, Sha256};

fn test_keypair() -> ([u8; 32], [u8; 33]) {
    let sk_bytes: [u8; 32] = Sha256::digest(b"pachi-vrf-bench-key").into();
    let sk = k256::SecretKey::from_bytes((&sk_bytes).into()).unwrap();
    let pk = sk.public_key();
    let pk_encoded = k256::elliptic_curve::sec1::ToEncodedPoint::to_encoded_point(&pk, true);
    let mut pk_bytes = [0u8; 33];
    pk_bytes.copy_from_slice(pk_encoded.as_bytes());
    (sk_bytes, pk_bytes)
}

fn bench_vrf(c: &mut Criterion) {
    let (sk, pk) = test_keypair();
    let seed = B256::from([0xABu8; 32]);
    let (random_value, proof) = vrf_compute(&sk, &seed).unwrap();

    c.bench_function("vrf_compute", |b| {
        b.iter(|| vrf_compute(&sk, &seed).unwrap());
    });

    c.bench_function("vrf_verify", |b| {
        b.iter(|| vrf_verify(&pk, &seed, &random_value, &proof).unwrap());
    });
}

criterion_group!(benches, bench_vrf);
criterion_main!(benches);
