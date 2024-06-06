// Copyright 2024 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::device::v4l2::stateless::buffer::V4l2CaptureDqBuf;
use crate::device::v4l2::stateless::buffer::V4l2CaptureQBuf;
use crate::device::v4l2::stateless::buffer::V4l2OutputDqBuf;
use crate::device::v4l2::stateless::buffer::V4l2OutputQBuf;
use crate::device::v4l2::stateless::device::V4l2Device;

use v4l2r::Format;
use v4l2r::PixelFormat;
use v4l2r::PlaneLayout;
use v4l2r::bindings::v4l2_format;
use v4l2r::device::AllocatedQueue;
use v4l2r::device::Stream;
use v4l2r::device::TryDequeue;
use v4l2r::device::queue::BuffersAllocated;
use v4l2r::device::queue::Queue;
use v4l2r::device::queue::direction::Capture;
use v4l2r::device::queue::direction::Output;
use v4l2r::device::queue::qbuf::get_free::GetFreeCaptureBuffer;
use v4l2r::device::queue::qbuf::get_free::GetFreeOutputBuffer;
use v4l2r::memory::MemoryType;
use v4l2r::memory::MmapHandle;

pub struct V4l2OutputQueue {
    // TODO: handle other queue states (AwaitingOutputFormat, AwaitingOutputBuffers, ReadyToDecode)
    // TODO: handle other memory backends
    pub handle: Queue<Output, BuffersAllocated<Vec<MmapHandle>>>,
}

impl V4l2OutputQueue {
    pub fn new(device: &V4l2Device, num_buffers: u32) -> Self {
        let mut handle = Queue::get_output_mplane_queue(device.video_device())
                .expect("Failed to get output queue");

        // TODO: handle other video formats at runtime
        // fmt.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_H264_SLICE;
        // fmt.fmt.pix_mp.width = (sps->pic_width_in_mbs_minus1 + 1) * mb_unit;
        // fmt.fmt.pix_mp.height = (sps->pic_height_in_map_units_minus1 + 1) * map_unit;
        // fmt.fmt.pix_mp.plane_fmt[0].sizeimage = 1024 * 1024; // ??? set by driver
        handle.change_format()
            .expect("Failed to change output format")
            .set_size(864, 480)
            .set_pixelformat(PixelFormat::from_fourcc(b"S264"))
            // 1 MB per decoding unit should be enough for most streams.
            .set_planes_layout(vec![PlaneLayout {
                sizeimage: 1024 * 1024,
                ..Default::default()
            }])
            .apply::<v4l2_format>()
            .expect("Failed to apply output format");

        let format: Format = handle.get_format().expect("Failed to get output format");
        println!(
            "Output format:\n\t{:?}\n", format
        );

        // TODO: handle other queue states at runtime
        let handle = handle
            .request_buffers_generic::<Vec<MmapHandle>>(MemoryType::Mmap, num_buffers)
            .expect("Failed to request output buffers");
        println!(
            "Output queue:\n\tnum_buffers: {}\n\tnum_queued_buffers: {}\n\tnum_free_buffers: {}\n",
            handle.num_buffers(), handle.num_queued_buffers(), handle.num_free_buffers()
        );

        // TODO: handle start/stop at runtime
        handle.stream_on()
            .expect("Failed to start output queue");

        Self {
            handle,
        }
    }

    pub fn num_buffers(&self) -> usize {
        let queue = &self.handle;
        queue.num_buffers()
    }

    pub fn num_free_buffers(&self) -> usize {
        let queue = &self.handle;
        queue.num_free_buffers()
    }

    pub fn alloc_buffer<'a>(&'a self) -> V4l2OutputQBuf<'a> {
        let queue = &self.handle;
        queue.try_get_free_buffer()
            .expect("Failed to alloc output buffer")
    }

    pub fn dequeue_buffer(&self) -> Option<V4l2OutputDqBuf> {
        let queue = &self.handle;
        match queue.try_dequeue() {
            Ok(buf) => Some(buf),
            _ => None,
        }
    }
}

pub struct V4l2CaptureQueue {
    // TODO: handle other queue states
    // TODO: handle other memory backends
    pub handle: Queue<Capture, BuffersAllocated<Vec<MmapHandle>>>,
}

impl V4l2CaptureQueue {
    pub fn new(device: &V4l2Device, num_buffers: u32) -> Self {
        let handle = Queue::get_capture_mplane_queue(device.video_device())
                .expect("Failed to get capture queue");

        // TODO: handle other video formats at runtime
        let format: Format = handle.get_format()
                .expect("Failed to get capture format");
        println!(
            "Capture format:\n\t{:?}\n", format
        );

        // TODO: handle other queue states at runtime
        let handle = handle
            .request_buffers_generic::<Vec<MmapHandle>>(MemoryType::Mmap, num_buffers)
            .expect("Failed to request capture buffers");
        println!(
            "Capture queue:\n\tnum_buffers: {}\n\tnum_queued_buffers: {}\n\tnum_free_buffers: {}\n",
            handle.num_buffers(), handle.num_queued_buffers(), handle.num_free_buffers()
        );

        // TODO: handle start/stop at runtime
        handle.stream_on()
            .expect("Failed to start capture queue");

        Self {
            handle,
        }
    }

    pub fn num_buffers(&self) -> usize {
        let queue = &self.handle;
        queue.num_buffers()
    }

    pub fn num_free_buffers(&self) -> usize {
        let queue = &self.handle;
        queue.num_free_buffers()
    }

    pub fn alloc_buffer<'a>(&'a self) -> V4l2CaptureQBuf<'a> {
        let queue = &self.handle;
        queue.try_get_free_buffer()
            .expect("Failed to alloc capture buffer")
    }

    pub fn dequeue_buffer(&self) -> Option<V4l2CaptureDqBuf> {
        let queue = &self.handle;
        match queue.try_dequeue() {
            Ok(buf) => Some(buf),
            _ => None,
        }
    }
}
