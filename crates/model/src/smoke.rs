use anyhow::{Context, Result, ensure};
use burn::{
    backend::{Autodiff, NdArray},
    tensor::{Tensor, TensorData},
};
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, path::Path};

type Cpu = NdArray<f32>;
type Train = Autodiff<Cpu>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema_version: u32,
    architecture: String,
    weights: Vec<f32>,
}

#[derive(Debug, Serialize)]
pub struct SmokeReport {
    pub evidence: String,
    pub backend: String,
    pub steps: usize,
    pub initial_loss: f32,
    pub final_loss: f32,
    pub nonzero_finite_gradient: bool,
    pub parameters_changed: bool,
    pub reload_max_error: f32,
}

/// A two-weight regression is intentionally an infrastructure check, not a
/// language-model training result or a pretrained-model compatibility claim.
pub fn train_smoke(checkpoint: &Path) -> Result<SmokeReport> {
    ensure!(
        !checkpoint.exists(),
        "checkpoint already exists; refusing overwrite"
    );
    let device = Default::default();
    let x = Tensor::<Train, 2>::from_floats([[1., 0.], [0., 1.], [1., 1.], [-1., 1.]], &device);
    let y = Tensor::<Train, 2>::from_floats([[1.], [2.], [3.], [1.]], &device);
    let mut weights = Tensor::<Train, 2>::from_floats([[0.1], [-0.1]], &device).require_grad();
    let mut initial_loss = 0.0;
    let mut gradient_ok = false;
    for step in 0..64 {
        let error = x.clone().matmul(weights.clone()) - y.clone();
        let loss = error.clone().mul(error).mean();
        let value = loss.clone().into_scalar();
        ensure!(value.is_finite(), "nonfinite smoke loss");
        if step == 0 {
            initial_loss = value;
        }
        let gradients = loss.backward();
        let gradient = weights.grad(&gradients).context("missing gradient")?;
        let values = gradient
            .to_data()
            .to_vec::<f32>()
            .map_err(|e| anyhow::anyhow!("gradient read: {e:?}"))?;
        ensure!(values.iter().all(|v| v.is_finite()), "nonfinite gradient");
        gradient_ok |= values.iter().any(|v| v.abs() > 1e-8);
        weights = Tensor::from_inner(weights.inner() - gradient.mul_scalar(0.1)).require_grad();
    }
    let trained = weights.inner();
    let values = trained
        .to_data()
        .to_vec::<f32>()
        .map_err(|e| anyhow::anyhow!("weight read: {e:?}"))?;
    let predictions = x.clone().inner().matmul(trained.clone());
    let error = predictions.clone() - y.inner();
    let final_loss = error.clone().mul(error).mean().into_scalar();
    let parameters_changed = values != vec![0.1, -0.1];
    ensure!(
        gradient_ok && parameters_changed && final_loss < initial_loss * 0.01,
        "smoke failed to learn fixture"
    );
    let artifact = Checkpoint {
        schema_version: 1,
        architecture: "synthetic-linear-2x1-not-an-llm".into(),
        weights: values,
    };
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(checkpoint)?;
    serde_json::to_writer_pretty(file, &artifact)?;
    let loaded: Checkpoint = serde_json::from_reader(std::fs::File::open(checkpoint)?)?;
    ensure!(
        loaded.schema_version == 1
            && loaded.architecture == artifact.architecture
            && loaded.weights.len() == 2,
        "invalid smoke checkpoint"
    );
    let restored = Tensor::<Cpu, 2>::from_data(TensorData::new(loaded.weights, [2, 1]), &device);
    let differences = (x.inner().matmul(restored) - predictions).abs();
    let reload_max_error = differences.max().into_scalar();
    ensure!(
        reload_max_error <= 1e-6,
        "checkpoint reload predictions changed"
    );
    Ok(SmokeReport {
        evidence: "infrastructure_only".into(),
        backend: "burn-0.21.0-ndarray-cpu-f32".into(),
        steps: 64,
        initial_loss,
        final_loss,
        nonzero_finite_gradient: gradient_ok,
        parameters_changed,
        reload_max_error,
    })
}
