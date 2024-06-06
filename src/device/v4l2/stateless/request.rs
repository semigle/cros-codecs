// Copyright 2024 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::device::v4l2::stateless::buffer::V4l2OutputQBuf;

use std::os::fd::AsRawFd;
use std::os::fd::RawFd;

use v4l2r::controls::ExtControlTrait;
use v4l2r::controls::SafeExtControl;
use v4l2r::ioctl::CtrlWhich;
use v4l2r::ioctl::Request;
use v4l2r::ioctl::s_ext_ctrls;
use v4l2r::nix::sys::time::TimeVal;

pub struct V4l2Request<'a> {
    device: RawFd,
    request: Request,
    timestamp: TimeVal,
    buf: V4l2OutputQBuf<'a>,
    length: usize,
}

impl<'a> V4l2Request<'a> {

    pub fn new(device: RawFd, request: Request, buf: V4l2OutputQBuf<'a>) -> Self {
        Self {
            device,
            request,
            buf,
            timestamp: TimeVal::new(0, 0),
            length: 0,
        }
    }

    pub fn set_timestamp(&mut self, timestamp: u64) -> &mut Self {
        self.timestamp = TimeVal::new(
            /* FIXME: sec */0, timestamp as i64);
        self
    }

    pub fn set_ctrl<C, T>(&mut self, ctrl: C) -> &mut Self
    where
        C: Into<SafeExtControl<T>>,
        T: ExtControlTrait,
    {
        let which = CtrlWhich::Request(self.request.as_raw_fd());
        let mut ctrl: SafeExtControl<T> = ctrl.into();
        s_ext_ctrls(&self.device, which, &mut ctrl)
            .expect("Failed to set output control");
        self
    }

    pub fn set_data(&mut self, data: &[u8]) -> &mut Self {

        let mut mapping = self.buf
            .get_plane_mapping(0)
            .expect("Failed to mmap output buffer");

        mapping.as_mut()[self.length..self.length+3]
            .copy_from_slice(&[0, 0, 1]);
        self.length += 3;

        mapping.as_mut()[self.length..self.length+data.len()]
            .copy_from_slice(data);
        self.length += data.len();

        drop(mapping);
        self
    }

    pub fn submit(self) {
        println!("output  >> index: {}, timestamp: {:?}\n",
            self.buf.index(), self.timestamp.as_ref());

        self.buf
            .set_request(self.request.as_raw_fd())
            .set_timestamp(self.timestamp)
            .queue(&[self.length])
            .expect("Failed to queue output buffer");

        self.request
            .queue()
            .expect("Failed to queue media request");
    }
}
