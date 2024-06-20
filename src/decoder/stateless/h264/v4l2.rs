// Copyright 2024 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::cell::RefCell;
use std::rc::Rc;

use crate::backend::v4l2::decoder::stateless::BackendHandle;
use crate::backend::v4l2::decoder::stateless::V4l2Picture;
use crate::backend::v4l2::decoder::stateless::V4l2StatelessDecoderBackend;
use crate::backend::v4l2::decoder::stateless::V4l2StatelessDecoderHandle;
use crate::codec::h264::dpb::Dpb;
use crate::codec::h264::dpb::DpbEntry;
use crate::codec::h264::parser::Pps;
use crate::codec::h264::parser::Slice;
use crate::codec::h264::parser::SliceHeader;
use crate::codec::h264::parser::Sps;
use crate::codec::h264::picture::PictureData;
use crate::decoder::stateless::h264::StatelessH264DecoderBackend;
use crate::decoder::stateless::h264::H264;
use crate::decoder::stateless::StatelessBackendResult;
use crate::decoder::stateless::StatelessDecoder;
use crate::decoder::stateless::StatelessDecoderBackendPicture;
use crate::decoder::BlockingMode;
use crate::device::v4l2::stateless::controls::h264::V4l2CtrlH264DecodeMode;
use crate::device::v4l2::stateless::controls::h264::V4l2CtrlH264DpbEntry;

impl StatelessDecoderBackendPicture<H264> for V4l2StatelessDecoderBackend {
    type Picture = Rc<RefCell<V4l2Picture>>;
}

impl StatelessH264DecoderBackend for V4l2StatelessDecoderBackend {

    fn new_sequence(&mut self, _: &Rc<Sps>) -> StatelessBackendResult<()> {
        Ok(())
    }

    fn new_picture(
        &mut self,
        _picture_data: &PictureData,
        timestamp: u64,
    ) -> StatelessBackendResult<Self::Picture> {
        let picture: Self::Picture = Rc::new(V4l2Picture {
            // TODO: general cleanup
            timestamp,
            buf: None,
            backend: self,
            h264_sps: Default::default(),
            h264_pps: Default::default(),
            h264_scaling_matrix: Default::default(),
            h264_decode_params: Default::default(),
        }.into());
        self.cur_pic_list.insert(timestamp, picture.clone());
        Ok(picture)
    }

    fn new_field_picture(
        &mut self,
        _picture_data: &PictureData,
        _timestamp: u64,
        _first_field: &Self::Handle,
    ) -> StatelessBackendResult<Self::Picture> {
        todo!()
    }

    fn start_picture(
        &mut self,
        picture: &mut Self::Picture,
        picture_data: &PictureData,
        sps: &Sps,
        pps: &Pps,
        dpb: &Dpb<Self::Handle>,
        slice_header: &SliceHeader,
    ) -> StatelessBackendResult<()> {

        let mut dpb_entries = Vec::<V4l2CtrlH264DpbEntry>::new();
        for entry in dpb.entries() {
            let ref_pic = match &entry.handle {
                Some(handle) => {
                    handle.handle.borrow().picture.clone()
                }
                None => todo!()
            };
            dpb_entries.push(V4l2CtrlH264DpbEntry {
                timestamp: ref_pic.borrow().timestamp, pic: entry.pic.clone()
            });
        }

        let mut picture = picture.borrow_mut();
        picture.h264_sps.set(sps);
        picture.h264_pps.set(pps);
        picture.h264_decode_params
            .set_picture_data(picture_data)
            .set_dpb_entries(&dpb_entries)
            .set_slice_header(slice_header);

        ////////////////////////////////////////////////////////////////////////
        // DEBUG
        ////////////////////////////////////////////////////////////////////////
        {
            let mut dpb_timestamps = Vec::<u64>::new();
            for entry in dpb.entries() {
                match &entry.handle {
                    Some(handle) => {
                        dpb_timestamps.push(handle.handle.borrow().picture.borrow().timestamp);
                    }
                    None => todo!(),
                };
            }
            println!("{:<20} {:?} {:?}\n", "start_picture",
                picture.timestamp, dpb_timestamps);
        }
        ////////////////////////////////////////////////////////////////////////

        Ok(())
    }

    fn decode_slice(
        &mut self,
        picture: &mut Self::Picture,
        slice: &Slice,
        _sps: &Sps,
        _pps: &Pps,
        _ref_pic_list0: &[&DpbEntry<Self::Handle>],
        _ref_pic_list1: &[&DpbEntry<Self::Handle>],
    ) -> StatelessBackendResult<()> {

        let picture = picture.borrow();

        // To do the following it would be ideally to emplace generic
        // request within picture, however, due to Output Buffer and
        // Output Queue lifetime constraints it's currently not possible
        // without changes to the generic backend framework.
        //
        // TODO: Move request allocation to new_picture
        // TODO: Move request control settings to start_picture
        // TODO: Move request submit to submit_picture

        let buf = self.output_queue.alloc_buffer();
        let mut request = self.device.alloc_request(buf);

        request.set_timestamp(picture.timestamp)
               .set_ctrl(&picture.h264_sps)
               .set_ctrl(&picture.h264_pps)
               .set_ctrl(&picture.h264_decode_params)
               .set_ctrl(V4l2CtrlH264DecodeMode::FrameBased)
               .set_data(slice.nalu.as_ref());

        request.submit();

        let buf = self.capture_queue.alloc_buffer();
        println!("capture >> index: {}\n", buf.index());
        buf.queue().expect("Failed to queue capture buffer");

        Ok(())

    }

    fn submit_picture(&mut self, picture: Self::Picture) -> StatelessBackendResult<Self::Handle> {
        let handle = Rc::new(RefCell::new(BackendHandle {
            picture: picture.clone(),
        }));
        println!("{:<20} {:?}\n", "submit_picture", picture.borrow().timestamp);
        Ok(V4l2StatelessDecoderHandle { handle })
    }
}

impl StatelessDecoder<H264, V4l2StatelessDecoderBackend> {
    // Creates a new instance of the decoder using the v4l2 backend.
    pub fn new_v4l2(blocking_mode: BlockingMode) -> Self {
        Self::new(V4l2StatelessDecoderBackend::new(), blocking_mode)
            .expect("Failed to create v4l2 stateless decoder backend")
    }
}
