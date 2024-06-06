// Copyright 2024 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::device::v4l2::stateless::buffer::V4l2OutputQBuf;
use crate::device::v4l2::stateless::request::V4l2Request;

use std::os::fd::AsRawFd;
use std::os::fd::RawFd;
use std::path::Path;
use std::sync::Arc;

use v4l2r::device::Device;
use v4l2r::device::DeviceConfig;
use v4l2r::ioctl::Request;
use v4l2r::nix::fcntl::OFlag;
use v4l2r::nix::fcntl::open;
use v4l2r::nix::sys::stat::Mode;

pub struct V4l2Device {
    video_device: Arc<Device>,
    media_device: RawFd,
}

impl V4l2Device {

    pub fn new() -> Self {

        // TODO: pass video device path and config via function arguments
        let video_device_path = Path::new("/dev/video-dec0");
        let video_device_config = DeviceConfig::new()
                .non_blocking_dqbuf();
        let video_device = Arc::new(Device::open(video_device_path, video_device_config)
                .expect("Failed to open video device"));

        // TODO: probe capabilties to find releted media device path
        let media_device_path = Path::new("/dev/media-dec0");
        let media_device = open(media_device_path, OFlag::O_RDWR | OFlag::O_CLOEXEC, Mode::empty())
                .unwrap_or_else(|_| panic!("Cannot open {}", media_device_path.display()));

        Self {
            video_device,
            media_device,
        }
    }

    pub fn video_device(&self) -> Arc<Device> {
        self.video_device.clone()
    }

    pub fn alloc_request<'a>(&self, buf: V4l2OutputQBuf<'a>) -> V4l2Request<'a> {

        let video_device = self.video_device.as_raw_fd();
        let media_request = Request::alloc(&self.media_device)
                .expect("Failed to alloc media request");

        V4l2Request::<'a>::new(
            video_device,
            media_request,
            buf
        )
    }
}
