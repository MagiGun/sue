//! The registry of models stored on this device.

use crate::{AiError, AiTask, ComputeBackend, ModelDescriptor};

/// A model paired with the backend chosen to run it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelSelection<'a> {
    /// The chosen model.
    pub model: &'a ModelDescriptor,
    /// The backend it will run on.
    pub backend: ComputeBackend,
}

/// Every model installed on the device. Models arrive as `AiModel` packages
/// through `sue-pkg`; nothing here touches the network.
#[derive(Clone, Debug, Default)]
pub struct LocalModelRegistry {
    models: Vec<ModelDescriptor>,
}

impl LocalModelRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a model.
    ///
    /// # Errors
    /// [`AiError::DuplicateModel`] if the id is taken.
    pub fn register(&mut self, model: ModelDescriptor) -> Result<(), AiError> {
        if self.get(&model.id).is_some() {
            return Err(AiError::DuplicateModel(model.id));
        }
        self.models.push(model);
        Ok(())
    }

    /// The model with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ModelDescriptor> {
        self.models.iter().find(|model| model.id == id)
    }

    /// Models that perform `task`, in registration order.
    pub fn models_for(&self, task: AiTask) -> impl Iterator<Item = &ModelDescriptor> {
        self.models.iter().filter(move |model| model.task == task)
    }

    /// Picks the model and backend for `task`: the best-ranked backend that is
    /// both available and supported, then the largest model (a proxy for
    /// quality) that fits `memory_budget`.
    #[must_use]
    pub fn select(
        &self,
        task: AiTask,
        available: &[ComputeBackend],
        memory_budget: u64,
    ) -> Option<ModelSelection<'_>> {
        self.models_for(task)
            .filter(|model| model.min_memory_bytes <= memory_budget)
            .filter_map(|model| {
                let backend = model
                    .backends
                    .iter()
                    .copied()
                    .filter(|backend| available.contains(backend))
                    .max_by_key(|backend| backend.rank())?;
                Some(ModelSelection { model, backend })
            })
            .max_by_key(|selection| (selection.backend.rank(), selection.model.size_bytes))
    }

    /// Number of models.
    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// `true` if no model is installed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelFormat;

    const GIB: u64 = 1 << 30;

    fn registry() -> LocalModelRegistry {
        let mut registry = LocalModelRegistry::new();
        let models = [
            ModelDescriptor::new("whisper-tiny", AiTask::Transcription, ModelFormat::Gguf)
                .with_size(75 << 20)
                .with_memory(GIB / 2),
            ModelDescriptor::new("whisper-large", AiTask::Transcription, ModelFormat::Gguf)
                .with_size(3 * GIB)
                .with_memory(6 * GIB)
                .with_quantization("Q5_K")
                .with_backends([ComputeBackend::Cuda, ComputeBackend::CpuSimd]),
            ModelDescriptor::new("whisper-medium", AiTask::Transcription, ModelFormat::Onnx)
                .with_size(GIB)
                .with_memory(2 * GIB)
                .with_backends([ComputeBackend::VulkanWgpu, ComputeBackend::CpuSimd]),
        ];
        for model in models {
            registry.register(model).unwrap();
        }
        registry
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut registry = registry();
        let duplicate =
            ModelDescriptor::new("whisper-tiny", AiTask::Transcription, ModelFormat::Gguf);
        assert_eq!(
            registry.register(duplicate),
            Err(AiError::DuplicateModel("whisper-tiny".into()))
        );
        assert_eq!(registry.len(), 3);
    }

    #[test]
    fn selects_by_backend_then_size_within_budget() {
        let registry = registry();
        let gpu_rig = [
            ComputeBackend::Cuda,
            ComputeBackend::VulkanWgpu,
            ComputeBackend::CpuSimd,
        ];
        let pick = registry
            .select(AiTask::Transcription, &gpu_rig, 16 * GIB)
            .unwrap();
        assert_eq!(
            (pick.model.id.as_str(), pick.backend),
            ("whisper-large", ComputeBackend::Cuda)
        );

        let pick = registry
            .select(AiTask::Transcription, &gpu_rig, 4 * GIB)
            .unwrap();
        assert_eq!(
            (pick.model.id.as_str(), pick.backend),
            ("whisper-medium", ComputeBackend::VulkanWgpu)
        );

        let pick = registry
            .select(AiTask::Transcription, &[ComputeBackend::CpuSimd], 16 * GIB)
            .unwrap();
        assert_eq!(pick.model.id, "whisper-large");

        assert!(
            registry
                .select(AiTask::Transcription, &gpu_rig, GIB / 4)
                .is_none()
        );
        assert!(registry.select(AiTask::Ocr, &gpu_rig, 16 * GIB).is_none());
    }
}
