//! ONNX models run on the processor, through `tract` (pure Rust, so it
//! builds for every platform Sue does, the web included). A model is
//! loaded once from its bytes and then run as often as needed, from any
//! thread.

use tract_onnx::prelude::*;

use crate::AiError;

/// One input tensor: its shape and values.
#[derive(Clone, Debug, PartialEq)]
pub enum OnnxInput {
    /// 32-bit floats.
    F32(Vec<usize>, Vec<f32>),
    /// 32-bit integers.
    I32(Vec<usize>, Vec<i32>),
}

/// One output tensor, as floats.
#[derive(Clone, Debug, PartialEq)]
pub struct OnnxOutput {
    /// Its shape.
    pub shape: Vec<usize>,
    /// Its values, row by row.
    pub values: Vec<f32>,
}

/// An ONNX model, optimized and ready to run.
pub struct OnnxModel {
    plan: std::sync::Arc<TypedRunnableModel>,
}

impl std::fmt::Debug for OnnxModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxModel").finish_non_exhaustive()
    }
}

fn failed(error: impl std::fmt::Display) -> AiError {
    AiError::Run(error.to_string())
}

impl OnnxModel {
    /// Reads and optimizes the model in `bytes` (an `.onnx` file).
    ///
    /// # Errors
    ///
    /// [`AiError::UnknownFormat`] when the bytes are not an ONNX model,
    /// [`AiError::Run`] when tract cannot plan it.
    pub fn load(bytes: &[u8]) -> Result<Self, AiError> {
        let model = tract_onnx::onnx()
            .model_for_read(&mut &bytes[..])
            .map_err(|_| AiError::UnknownFormat)?;
        let plan = model
            .into_optimized()
            .and_then(|model| model.into_runnable())
            .map_err(failed)?;
        Ok(Self { plan })
    }

    /// Runs the model on `inputs`, in the model's input order.
    ///
    /// # Errors
    ///
    /// [`AiError::ShapeMismatch`] when an input's values do not fill its
    /// shape, [`AiError::Run`] when the model cannot run on them.
    pub fn run(&self, inputs: Vec<OnnxInput>) -> Result<Vec<OnnxOutput>, AiError> {
        let mut values: TVec<TValue> = TVec::new();
        for input in inputs {
            let tensor: Tensor = match input {
                OnnxInput::F32(shape, data) => tract_ndarray::ArrayD::from_shape_vec(shape, data)
                    .map_err(|_| AiError::ShapeMismatch("input values"))?
                    .into(),
                OnnxInput::I32(shape, data) => tract_ndarray::ArrayD::from_shape_vec(shape, data)
                    .map_err(|_| AiError::ShapeMismatch("input values"))?
                    .into(),
            };
            values.push(tensor.into());
        }
        let outputs = self.plan.run(values).map_err(failed)?;
        outputs
            .iter()
            .map(|output| {
                let output = output.cast_to::<f32>().map_err(failed)?;
                let view = output.to_plain_array_view::<f32>().map_err(failed)?;
                Ok(OnnxOutput {
                    shape: view.shape().to_vec(),
                    values: view.iter().copied().collect(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_not_onnx_is_refused() {
        assert_eq!(
            OnnxModel::load(b"not a model").unwrap_err(),
            AiError::UnknownFormat
        );
    }
}
