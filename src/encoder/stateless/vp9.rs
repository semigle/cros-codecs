// Copyright 2024 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::rc::Rc;

use crate::codec::vp9::parser::Header;
use crate::encoder::stateless::vp9::predictor::LowDelayVP9;
use crate::encoder::stateless::BitstreamPromise;
use crate::encoder::stateless::Predictor;
use crate::encoder::stateless::StatelessBackendResult;
use crate::encoder::stateless::StatelessCodec;
use crate::encoder::stateless::StatelessEncoderExecute;
use crate::encoder::stateless::StatelessVideoEncoderBackend;
use crate::encoder::vp9::EncoderConfig;
use crate::encoder::vp9::VP9;
use crate::encoder::EncodeResult;
use crate::encoder::FrameMetadata;
use crate::encoder::PredictionStructure;
use crate::encoder::Tunings;
use crate::BlockingMode;

mod predictor;

#[cfg(feature = "vaapi")]
pub mod vaapi;

/// Determines how reference frame shall be used
pub enum ReferenceUse {
    /// The frame will be used for single prediction
    Single,
    /// The frame will be used for compound prediction
    Compound,
    /// The frame will be used for both single and compound prediction
    Hybrid,
}

pub struct BackendRequest<P, R> {
    header: Header,

    /// Input frame to be encoded
    input: P,

    /// Input frame metadata
    input_meta: FrameMetadata,

    /// Reference frames
    last_frame_ref: Option<(Rc<R>, ReferenceUse)>,
    golden_frame_ref: Option<(Rc<R>, ReferenceUse)>,
    altref_frame_ref: Option<(Rc<R>, ReferenceUse)>,

    /// [`Tunings`] for the frame
    tunings: Tunings,

    /// Container for the request output. [`StatelessVP9EncoderBackend`] impl shall move it and
    /// append the slice data to it. This prevents unnecessary copying of bitstream around.
    coded_output: Vec<u8>,
}

impl<Backend> StatelessCodec<Backend> for VP9
where
    Backend: StatelessVideoEncoderBackend<VP9>,
{
    type Reference = Backend::Reconstructed;

    type Request = BackendRequest<Backend::Picture, Backend::Reconstructed>;

    type CodedPromise = BitstreamPromise<Backend::CodedPromise>;

    type ReferencePromise = Backend::ReconPromise;
}

pub trait StatelessVP9EncoderBackend: StatelessVideoEncoderBackend<VP9> {
    fn encode_frame(
        &mut self,
        request: BackendRequest<Self::Picture, Self::Reconstructed>,
    ) -> StatelessBackendResult<(Self::ReconPromise, Self::CodedPromise)>;
}

pub type StatelessEncoder<Handle, Backend> =
    crate::encoder::stateless::StatelessEncoder<VP9, Handle, Backend>;

impl<Handle, Backend> StatelessEncoderExecute<VP9, Handle, Backend>
    for StatelessEncoder<Handle, Backend>
where
    Backend: StatelessVP9EncoderBackend,
{
    fn execute(
        &mut self,
        request: BackendRequest<Backend::Picture, Backend::Reconstructed>,
    ) -> EncodeResult<()> {
        let meta = request.input_meta.clone();

        // The [`BackendRequest`] has a frame from predictor. Decresing internal counter.
        self.predictor_frame_count -= 1;

        log::trace!("submitting new request");
        let (recon, bitstream) = self.backend.encode_frame(request)?;

        // Wrap promise from backend with headers and metadata
        let slice_promise = BitstreamPromise { bitstream, meta };

        self.output_queue.add_promise(slice_promise);

        self.recon_queue.add_promise(recon);

        Ok(())
    }
}

impl<Handle, Backend> StatelessEncoder<Handle, Backend>
where
    Backend: StatelessVP9EncoderBackend,
{
    fn new_vp9(backend: Backend, config: EncoderConfig, mode: BlockingMode) -> EncodeResult<Self> {
        let predictor: Box<dyn Predictor<_, _, _>> = match config.pred_structure {
            PredictionStructure::LowDelay { limit } => Box::new(LowDelayVP9::new(config, limit)),
        };

        Self::new(backend, mode, predictor)
    }
}
