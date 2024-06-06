// Copyright 2024 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use v4l2r::device::queue::direction::Capture;
use v4l2r::device::queue::direction::Output;
use v4l2r::device::queue::dqbuf::DqBuffer;
use v4l2r::device::queue::qbuf::QBuffer;
use v4l2r::memory::MmapHandle;

// TODO: handle other memory backends
pub type V4l2OutputQBuf<'a> = QBuffer<'a, Output, Vec<MmapHandle>, Vec<MmapHandle>>;
pub type V4l2OutputDqBuf = DqBuffer<Output, Vec<MmapHandle>>;

// TODO: handle other memory backends
pub type V4l2CaptureQBuf<'a> = QBuffer<'a, Capture, Vec<MmapHandle>, Vec<MmapHandle>>;
pub type V4l2CaptureDqBuf = DqBuffer<Capture, Vec<MmapHandle>>;
