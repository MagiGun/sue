//! Zero-copy tensor tasks over arena memory.

use sue_core::{Arena, AssetHandle, ComponentData, CoreError};

use crate::{AiError, AiTask, ComputeBackend, ModelDescriptor, ModelFormat};

/// What a task run did. `bytes_copied` is part of the contract and is `0` for
/// conforming kernels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskReport {
    /// Backend the kernel ran on.
    pub backend: ComputeBackend,
    /// Input elements read.
    pub elements_read: usize,
    /// Output elements written.
    pub elements_written: usize,
    /// Bytes copied between buffers.
    pub bytes_copied: usize,
}

/// A kernel that reads one arena entity and writes another, in place.
pub trait ZeroCopyTensorTask: Send + Sync {
    /// Kernel id.
    fn name(&self) -> &str;

    /// The task, which fixes the input and output component kinds.
    fn task(&self) -> AiTask;

    /// Backends this kernel can execute on.
    fn supported_backends(&self) -> &[ComputeBackend];

    /// Runs the kernel on borrowed components.
    ///
    /// # Errors
    /// [`AiError::ShapeMismatch`] or kernel-specific errors.
    fn compute(
        &self,
        input: &ComponentData,
        output: &mut ComponentData,
        backend: ComputeBackend,
    ) -> Result<TaskReport, AiError>;

    /// Runs the kernel from `input` to `output` inside `arena`: picks the best
    /// backend in `available`, borrows both entities at once with
    /// [`Arena::split_pair`], and computes directly on their buffers.
    ///
    /// # Errors
    /// [`AiError::NoBackend`], arena errors (stale or aliased handles, missing
    /// components), or the kernel's own errors.
    fn run(
        &self,
        arena: &mut Arena,
        input: AssetHandle,
        output: AssetHandle,
        available: &[ComputeBackend],
    ) -> Result<TaskReport, AiError> {
        let backend = self
            .supported_backends()
            .iter()
            .copied()
            .filter(|backend| available.contains(backend))
            .max_by_key(|backend| backend.rank())
            .ok_or_else(|| AiError::NoBackend {
                task: self.name().to_owned(),
            })?;
        let task = self.task();
        let (source, target) = arena.split_pair(input, output)?;
        let input_component =
            source
                .component(task.input_kind())
                .ok_or(CoreError::MissingComponent {
                    handle: input,
                    kind: task.input_kind(),
                })?;
        let output_component =
            target
                .component_mut(task.output_kind())
                .ok_or(CoreError::MissingComponent {
                    handle: output,
                    kind: task.output_kind(),
                })?;
        self.compute(input_component, output_component, backend)
    }
}

/// Reference stem separator: splits stereo into a center stem
/// `M = (L + R) / 2` (lead vocal, bass, kick) and a side stem `S = (L − R) / 2`
/// (ambience, wide instruments), written as the output's left and right
/// channels. Exactly invertible: `L = M + S`, `R = M − S`.
#[derive(Clone, Copy, Debug, Default)]
pub struct MidSideStemSplit;

impl MidSideStemSplit {
    /// Kernel id.
    pub const ID: &'static str = "builtin.mid-side-stems";

    /// The registry entry for this built-in kernel.
    #[must_use]
    pub fn descriptor() -> ModelDescriptor {
        ModelDescriptor::new(Self::ID, AiTask::StemSeparation, ModelFormat::Builtin)
    }
}

impl ZeroCopyTensorTask for MidSideStemSplit {
    fn name(&self) -> &str {
        Self::ID
    }

    fn task(&self) -> AiTask {
        AiTask::StemSeparation
    }

    fn supported_backends(&self) -> &[ComputeBackend] {
        &[ComputeBackend::CpuSimd]
    }

    fn compute(
        &self,
        input: &ComponentData,
        output: &mut ComponentData,
        backend: ComputeBackend,
    ) -> Result<TaskReport, AiError> {
        let source = input
            .as_pcm()
            .ok_or(AiError::ShapeMismatch("input must be PCM audio"))?;
        let target = output
            .as_pcm_mut()
            .ok_or(AiError::ShapeMismatch("output must be PCM audio"))?;
        if source.channels() != 2 || target.channels() != 2 {
            return Err(AiError::ShapeMismatch(
                "mid/side needs stereo input and output",
            ));
        }
        if source.frames() != target.frames() || source.sample_rate() != target.sample_rate() {
            return Err(AiError::ShapeMismatch(
                "input and output must have the same length and sample rate",
            ));
        }
        // Both buffers are validated stereo, so neither has a remainder.
        let (frames_in, _) = source.samples().as_chunks::<2>();
        let (frames_out, _) = target.samples_mut().as_chunks_mut::<2>();
        for (&[left, right], stems) in frames_in.iter().zip(frames_out) {
            *stems = [0.5 * (left + right), 0.5 * (left - right)];
        }
        Ok(TaskReport {
            backend,
            elements_read: source.samples().len(),
            elements_written: target.samples().len(),
            bytes_copied: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{ComponentKind, PCMFloat1D, StringBuffer};

    fn stereo(samples: Vec<f32>) -> PCMFloat1D {
        PCMFloat1D::new(48_000, 2, samples).unwrap()
    }

    #[test]
    fn splits_and_inverts_exactly() {
        let mut arena = Arena::new();
        let mix = arena.spawn("mix.flac", stereo(vec![1.0, 1.0, 0.5, -0.5, 0.25, 0.75]));
        let stems = arena.spawn("stems", PCMFloat1D::silence(48_000, 2, 3).unwrap());
        let address = arena
            .get(stems)
            .unwrap()
            .component(ComponentKind::PCMFloat1D)
            .unwrap()
            .f32_view()
            .unwrap()
            .as_ptr();

        let report = MidSideStemSplit
            .run(&mut arena, mix, stems, &[ComputeBackend::CpuSimd])
            .unwrap();
        assert_eq!(report.bytes_copied, 0);
        assert_eq!((report.elements_read, report.elements_written), (6, 6));

        let output = arena
            .get(stems)
            .unwrap()
            .component(ComponentKind::PCMFloat1D)
            .unwrap();
        assert_eq!(output.f32_view().unwrap().as_ptr(), address);
        let samples = output.as_pcm().unwrap().samples();
        assert_eq!(samples, [1.0, 0.0, 0.0, 0.5, 0.5, -0.25]);
        let input = arena
            .get(mix)
            .unwrap()
            .component(ComponentKind::PCMFloat1D)
            .unwrap();
        for (frame, stem) in input
            .as_pcm()
            .unwrap()
            .samples()
            .chunks(2)
            .zip(samples.chunks(2))
        {
            assert_eq!(frame[0], stem[0] + stem[1]);
            assert_eq!(frame[1], stem[0] - stem[1]);
        }
    }

    #[test]
    fn rejects_bad_shapes_and_handles() {
        let mut arena = Arena::new();
        let mono = arena.spawn("mono", PCMFloat1D::new(48_000, 1, vec![0.0; 4]).unwrap());
        let out = arena.spawn("out", PCMFloat1D::silence(48_000, 2, 2).unwrap());
        let cpu = [ComputeBackend::CpuSimd];
        assert!(matches!(
            MidSideStemSplit.run(&mut arena, mono, out, &cpu),
            Err(AiError::ShapeMismatch(_))
        ));

        let text = arena.spawn("text", StringBuffer::from("not audio"));
        assert!(matches!(
            MidSideStemSplit.run(&mut arena, text, out, &cpu),
            Err(AiError::Core(CoreError::MissingComponent { .. }))
        ));
        assert!(matches!(
            MidSideStemSplit.run(&mut arena, out, out, &cpu),
            Err(AiError::Core(CoreError::AliasedHandles(_)))
        ));
        assert!(matches!(
            MidSideStemSplit.run(&mut arena, mono, out, &[ComputeBackend::Metal]),
            Err(AiError::NoBackend { .. })
        ));
        let short = arena.spawn("short", PCMFloat1D::silence(48_000, 2, 1).unwrap());
        assert!(MidSideStemSplit.run(&mut arena, out, short, &cpu).is_err());
    }

    #[test]
    fn describes_itself_for_the_registry() {
        let descriptor = MidSideStemSplit::descriptor();
        assert_eq!(descriptor.format, ModelFormat::Builtin);
        assert_eq!(descriptor.backends, [ComputeBackend::CpuSimd]);
    }
}
