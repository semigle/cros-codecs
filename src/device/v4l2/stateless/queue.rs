// Copyright 2024 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::device::v4l2::stateless::buffer::V4l2CaptureDqBuf;
use crate::device::v4l2::stateless::buffer::V4l2CaptureQBuf;
use crate::device::v4l2::stateless::buffer::V4l2OutputDqBuf;
use crate::device::v4l2::stateless::buffer::V4l2OutputQBuf;
use crate::device::v4l2::stateless::device::V4l2Device;
use crate::Resolution;

use v4l2r::Format;
use v4l2r::PixelFormat;
use v4l2r::PlaneLayout;
use v4l2r::bindings::v4l2_format;
use v4l2r::device::AllocatedQueue;
use v4l2r::device::Stream;
use v4l2r::device::TryDequeue;
use v4l2r::device::queue::BuffersAllocated;
use v4l2r::device::queue::Queue;
use v4l2r::device::queue::QueueInit;
use v4l2r::device::queue::direction::Capture;
use v4l2r::device::queue::direction::Output;
use v4l2r::device::queue::qbuf::get_free::GetFreeCaptureBuffer;
use v4l2r::device::queue::qbuf::get_free::GetFreeOutputBuffer;
use v4l2r::memory::MemoryType;
use v4l2r::memory::MmapHandle;

// TODO: handle other memory backends
enum OutputQueueState {
    Init(Queue<Output, QueueInit>),
    Streaming(Queue<Output, BuffersAllocated<Vec<MmapHandle>>>),
}

pub struct V4l2OutputQueue {
    handle: OutputQueueState,
    num_buffers: u32,
}

impl V4l2OutputQueue {
    pub fn new(device: &V4l2Device, num_buffers: u32) -> Self {
        let handle = Queue::get_output_mplane_queue(device.video_device())
                .expect("Failed to get output queue");
        println!("Output queue:\n\tstate: None -> Init\n");
        let handle = OutputQueueState::Init(handle);
        Self {
            handle,
            num_buffers,
        }
    }

    pub fn set_resolution(self, res: Resolution) -> Self {
        let handle = match self.handle {
            OutputQueueState::Init(mut handle) => {
                let (width, height) = res.into();

                handle.change_format()
                    .expect("Failed to change output format")
                    .set_size(width as usize, height as usize)
                    .set_pixelformat(PixelFormat::from_fourcc(b"S264"))
                    // 1 MB per decoding unit should be enough for most streams.
                    .set_planes_layout(vec![PlaneLayout {
                        sizeimage: 1024 * 1024,
                        ..Default::default()
                    }])
                    .apply::<v4l2_format>()
                    .expect("Failed to apply output format");

                let format: Format = handle.get_format()
                        .expect("Failed to get output format");
                println!(
                    "Output format:\n\t{:?}\n", format
                );

                let handle = handle
                    .request_buffers_generic::<Vec<MmapHandle>>(MemoryType::Mmap, self.num_buffers)
                    .expect("Failed to request output buffers");
                println!(
                    "Output queue:\n\tnum_buffers: {}\n\tnum_queued_buffers: {}\n\tnum_free_buffers: {}\n",
                    handle.num_buffers(), handle.num_queued_buffers(), handle.num_free_buffers()
                );

                // TODO: handle start/stop at runtime
                handle.stream_on()
                    .expect("Failed to start output queue");

                println!("Output queue:\n\tstate: Init -> Streaming\n");
                OutputQueueState::Streaming(handle)
            },
            _ => {
                /* TODO: handle DRC */
                 todo!()
            }
        };
        let num_buffers = self.num_buffers;
        Self {
            handle,
            num_buffers,
        }
    }

    pub fn num_buffers(&self) -> usize {
        match &self.handle {
            OutputQueueState::Streaming(handle) =>
                handle.num_buffers(),
            _ => todo!()
        }
    }

    pub fn num_free_buffers(&self) -> usize {
        match &self.handle {
            OutputQueueState::Streaming(handle) =>
                handle.num_free_buffers(),
            _ => todo!()
        }
    }

    pub fn alloc_buffer<'a>(&'a self) -> V4l2OutputQBuf<'a> {
        match &self.handle {
            OutputQueueState::Streaming(handle) =>
                handle.try_get_free_buffer()
                    .expect("Failed to alloc output buffer"),
            _ => todo!()
        }
    }

    pub fn dequeue_buffer(&self) -> Option<V4l2OutputDqBuf> {
        match &self.handle {
            OutputQueueState::Streaming(handle) =>
                match handle.try_dequeue() {
                    Ok(buf) => Some(buf),
                    _ => None,
                }
            _ => todo!()
        }
    }
}

// TODO: handle other memory backends
enum CaptureQueueState {
    Init(Queue<Capture, QueueInit>),
    Streaming(Queue<Capture, BuffersAllocated<Vec<MmapHandle>>>),
}

pub struct V4l2CaptureQueue {
    handle: CaptureQueueState,
    num_buffers: u32,
}

impl V4l2CaptureQueue {
    pub fn new(device: &V4l2Device, num_buffers: u32) -> Self {
        let handle = Queue::get_capture_mplane_queue(device.video_device())
                .expect("Failed to get capture queue");
        println!("Capture queue:\n\tstate: None -> Init\n");
        let handle = CaptureQueueState::Init(handle);
        Self {
            handle,
            num_buffers,
        }
    }

    pub fn set_resolution(self, _: Resolution) -> Self {
        let handle = match self.handle {
            CaptureQueueState::Init(handle) => {
                let format: Format = handle.get_format()
                        .expect("Failed to get capture format");
                println!(
                    "Capture format:\n\t{:?}\n", format
                );

                let handle = handle
                    .request_buffers_generic::<Vec<MmapHandle>>(MemoryType::Mmap, self.num_buffers)
                    .expect("Failed to request capture buffers");
                println!(
                    "Capture queue:\n\tnum_buffers: {}\n\tnum_queued_buffers: {}\n\tnum_free_buffers: {}\n",
                    handle.num_buffers(), handle.num_queued_buffers(), handle.num_free_buffers()
                );

                // TODO: handle start/stop at runtime
                handle.stream_on()
                    .expect("Failed to start capture queue");

                println!("Capture queue:\n\tstate: Init -> Streaming\n");
                CaptureQueueState::Streaming(handle)
            },
            _ => {
                /* TODO: handle DRC */
                 todo!()
            }
        };
        let num_buffers = self.num_buffers;
        Self {
            handle,
            num_buffers,
        }
    }

    pub fn num_buffers(&self) -> usize {
        match &self.handle {
            CaptureQueueState::Streaming(handle) =>
                handle.num_buffers(),
            _ => todo!()
        }
    }

    pub fn num_free_buffers(&self) -> usize {
        match &self.handle {
            CaptureQueueState::Streaming(handle) =>
                handle.num_free_buffers(),
            _ => todo!()
        }
    }

    pub fn alloc_buffer<'a>(&'a self) -> V4l2CaptureQBuf<'a> {
        match &self.handle {
            CaptureQueueState::Streaming(handle) =>
                handle.try_get_free_buffer()
                    .expect("Failed to alloc capture buffer"),
            _ => todo!()
        }
    }

    pub fn dequeue_buffer(&self) -> Option<V4l2CaptureDqBuf> {
        match &self.handle {
            CaptureQueueState::Streaming(handle) =>
                match handle.try_dequeue() {
                    Ok(buf) => Some(buf),
                    _ => None,
                }
            _ => todo!()
        }
    }
}
