// Copyright 2024 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::cell::RefCell;
use std::rc::Rc;
use std::collections::HashMap;

use crate::decoder::stateless::PoolLayer;
use crate::decoder::stateless::StatelessCodec;
use crate::decoder::stateless::StatelessDecoderBackend;
use crate::decoder::stateless::TryFormat;
use crate::decoder::DecodedHandle;
use crate::decoder::DynHandle;
use crate::decoder::FramePool;
use crate::decoder::MappableHandle;
use crate::decoder::StreamInfo;
use crate::DecodedFormat;
use crate::Resolution;

use crate::device::v4l2::stateless::buffer::V4l2CaptureDqBuf;
use crate::device::v4l2::stateless::controls::h264::V4l2CtrlH264DecodeParams;
use crate::device::v4l2::stateless::controls::h264::V4l2CtrlH264Pps;
use crate::device::v4l2::stateless::controls::h264::V4l2CtrlH264ScalingMatrix;
use crate::device::v4l2::stateless::controls::h264::V4l2CtrlH264Sps;
use crate::device::v4l2::stateless::device::V4l2Device;
use crate::device::v4l2::stateless::queue::V4l2CaptureQueue;
use crate::device::v4l2::stateless::queue::V4l2OutputQueue;

pub struct V4l2Picture {
    // TODO: consider enum to handle generic picture states
    pub timestamp: u64,
    pub buf: Option<V4l2CaptureDqBuf>,
    pub ref_pic_list: Vec<Rc<RefCell<V4l2Picture>>>,

    // FIXME: This is temporary hack to ensure picture ready
    pub backend: *mut V4l2StatelessDecoderBackend,

    // TODO: These are H264 codec specific data only.
    pub h264_sps: V4l2CtrlH264Sps,
    pub h264_pps: V4l2CtrlH264Pps,
    pub h264_scaling_matrix: V4l2CtrlH264ScalingMatrix,
    pub h264_decode_params: V4l2CtrlH264DecodeParams,
}

impl<'a> MappableHandle for std::cell::Ref<'a, V4l2Picture> {
    fn read(&mut self, data: &mut [u8]) -> anyhow::Result<()> {
        let buf = self.buf.as_ref()
            .expect("Failed to get capture buffer");
        let mut offset = 0;
        for i in 0..buf.data.num_planes() {
            let mapping = buf
                .get_plane_mapping(i)
                .expect("Failed to mmap capture buffer");
            data[offset..offset+mapping.size()].copy_from_slice(&mapping);
            offset += mapping.size();
            drop(mapping);
        }
        Ok(())
    }
    fn image_size(&mut self) -> usize {
        let buf = self.buf.as_ref()
            .expect("Failed to get capture buffer");
        let mut size = 0;
        for i in 0..buf.data.num_planes() {
            let mapping = buf
                .get_plane_mapping(i)
                .expect("Failed to mmap capture buffer");
            size += mapping.size();
            drop(mapping);
        }
        size
    }
}

pub struct BackendHandle {
    pub picture: Rc<RefCell<V4l2Picture>>,
}

impl<'a> DynHandle for std::cell::Ref<'a, BackendHandle> {
    fn dyn_mappable_handle<'b>(&'b self) -> anyhow::Result<Box<dyn MappableHandle + 'b>> {

        // FIXME: This is temporary hack to ensure picture ready
        loop {
            if let Some(_) = self.picture.borrow().buf {
                break;
            }
            println!("{:<20} {:?}\n", "dyn_mappable_handle", "sync");
            let backend: *mut V4l2StatelessDecoderBackend;
            {
                backend = self.picture.borrow().backend;
            }
            unsafe{&mut *backend}.process_capture_queue();
        }

        Ok(Box::new(self.picture.borrow()))
    }
}

pub struct V4l2StatelessDecoderHandle {
    pub handle: Rc<RefCell<BackendHandle>>,
}

impl Clone for V4l2StatelessDecoderHandle {
    fn clone(&self) -> Self {
        Self {
            handle: Rc::clone(&self.handle),
        }
    }
}

impl DecodedHandle for V4l2StatelessDecoderHandle {
    type Descriptor = ();

    fn coded_resolution(&self) -> Resolution {
        todo!();
    }

    fn display_resolution(&self) -> Resolution {
        todo!();
    }

    fn timestamp(&self) -> u64 {
        self.handle.borrow().picture.borrow().timestamp
    }

    fn dyn_picture<'a>(&'a self) -> Box<dyn DynHandle + 'a> {
        Box::new(self.handle.borrow())
    }

    fn sync(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn is_ready(&self) -> bool {
        todo!();
    }

    fn resource(&self) -> std::cell::Ref<()> {
        todo!();
    }
}

pub struct V4l2StatelessDecoderBackend {
    stream_info: StreamInfo,

    pub device: V4l2Device,
    pub output_queue: V4l2OutputQueue,
    pub capture_queue: V4l2CaptureQueue,

    // FIXME: This is temporary hack to ensure picture ready
    pub cur_pic_list: HashMap<u64, Rc<RefCell<V4l2Picture>>>,
}

impl V4l2StatelessDecoderBackend {

    pub fn new() -> Self {
        const NUM_OUTPUT_BUFFERS: u32 = 8;
        const NUM_CAPTURE_BUFFERS: u32 = 8;

        let device = V4l2Device::new();
        let output_queue = V4l2OutputQueue::new(&device, NUM_OUTPUT_BUFFERS);
        let capture_queue = V4l2CaptureQueue::new(&device, NUM_CAPTURE_BUFFERS);

        Self {
            stream_info: StreamInfo {
                format: DecodedFormat::I420,
                min_num_frames: 4,
                coded_resolution: Resolution::from((320, 200)),
                display_resolution: Resolution::from((320, 200)),
            },
            device,
            output_queue,
            capture_queue,

            // FIXME: This is temporary hack to ensure picture ready
            cur_pic_list: HashMap::<u64, Rc<RefCell<V4l2Picture>>>::new(),
        }
    }

    pub fn process_output_queue(&self) {
        let queue = &self.output_queue;
        loop {
            match queue.dequeue_buffer() {
                Some(buf) => {
                    println!("output  << index: {}, timestamp: {:?}\n", buf.data.index(), buf.data.timestamp());
                }
                _ => break,
            }
        }
    }

    pub fn process_capture_queue(&mut self) {
        let queue = &self.capture_queue;
        loop {
            match queue.dequeue_buffer() {
                Some(buf) => {
                    println!("capture << index: {}, timestamp: {:?}\n", buf.data.index(), buf.data.timestamp());

                    // FIXME: This is temporary hack to ensure picture ready
                    match self.cur_pic_list.remove(&(buf.data.timestamp().tv_usec as u64)) {
                         Some(picture) => {
                             picture.borrow_mut().buf = Some(buf);
                             picture.borrow_mut().ref_pic_list.clear();
                         }
                         _ => todo!(),
                    }
                }
                _ => break,
            }
        }
        while self.capture_queue.num_free_buffers() != 0 {
            let buf = self.capture_queue.alloc_buffer();
            println!("capture >> index: {}\n", buf.index());
            buf.queue().expect("Failed to queue capture buffer");
        }
    }
}

impl FramePool for V4l2StatelessDecoderBackend {
    type Descriptor = ();

    fn coded_resolution(&self) -> Resolution {
        todo!();
    }

    fn set_coded_resolution(&mut self, _resolution: Resolution) {
        todo!();
    }

    fn add_frames(&mut self, _descriptors: Vec<Self::Descriptor>) -> Result<(), anyhow::Error> {
        todo!();
    }

    fn num_free_frames(&self) -> usize {
        self.output_queue.num_free_buffers()
    }

    fn num_managed_frames(&self) -> usize {
        self.output_queue.num_buffers()
    }

    fn clear(&mut self) {
        todo!();
    }
}

impl<Codec: StatelessCodec> TryFormat<Codec> for V4l2StatelessDecoderBackend {
    fn try_format(&mut self, _: &Codec::FormatInfo, _: DecodedFormat) -> anyhow::Result<()> {
        // TODO
        Ok(())
    }
}

impl StatelessDecoderBackend for V4l2StatelessDecoderBackend {
    type Handle = V4l2StatelessDecoderHandle;

    type FramePool = Self;

    fn stream_info(&self) -> Option<&StreamInfo> {
        // TODO
        Some(&self.stream_info)
    }

    fn frame_pool(&mut self, _: PoolLayer) -> Vec<&mut Self::FramePool> {
        self.process_output_queue();
        self.process_capture_queue();
        vec![self]
    }
}
