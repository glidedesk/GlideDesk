// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// twindesk-vcam <device>: the virtual camera's writer on Linux (linux/vcam.js). Frames come on
// stdin, each 1280×720 NV12 (shared/vcam-feed.js); the newest goes to the v4l2loopback device as
// YUYV, which every app reads, 30 times a second. With none for a while (no camera shown): black.
// It writes all the while it runs: with exclusive_caps, apps see the device as a camera only while
// something writes to it.
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <time.h>
#include <unistd.h>
#include <linux/videodev2.h>

#define W 1280
#define H 720
#define FPS 30
#define NV12 (W * H * 3 / 2)
#define YUYV (W * H * 2)
#define STALE_MS 1500

static int64_t now_ms(void) {
  struct timespec t;
  clock_gettime(CLOCK_MONOTONIC, &t);
  return (int64_t)t.tv_sec * 1000 + t.tv_nsec / 1000000;
}

static void to_yuyv(const uint8_t *nv12, uint8_t *out) {
  const uint8_t *uv = nv12 + W * H;
  for (int y = 0; y < H; y++) {
    const uint8_t *yr = nv12 + y * W;
    const uint8_t *cr = uv + (y / 2) * W;
    uint8_t *o = out + y * W * 2;
    for (int x = 0; x < W; x += 2) {
      o[0] = yr[x];
      o[1] = cr[x];
      o[2] = yr[x + 1];
      o[3] = cr[x + 1];
      o += 4;
    }
  }
}

static void black(uint8_t *out) {
  for (int i = 0; i < YUYV; i += 2) {
    out[i] = 16;
    out[i + 1] = 128;
  }
}

int main(int argc, char **argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: twindesk-vcam /dev/videoN\n");
    return 2;
  }
  int fd = open(argv[1], O_WRONLY);
  if (fd < 0) {
    fprintf(stderr, "open %s: %s\n", argv[1], strerror(errno));
    return 1;
  }
  struct v4l2_format f;
  memset(&f, 0, sizeof f);
  f.type = V4L2_BUF_TYPE_VIDEO_OUTPUT;
  f.fmt.pix.width = W;
  f.fmt.pix.height = H;
  f.fmt.pix.pixelformat = V4L2_PIX_FMT_YUYV;
  f.fmt.pix.field = V4L2_FIELD_NONE;
  f.fmt.pix.bytesperline = W * 2;
  f.fmt.pix.sizeimage = YUYV;
  f.fmt.pix.colorspace = V4L2_COLORSPACE_SMPTE170M; // BT.601, as vcam-feed.js makes it
  if (ioctl(fd, VIDIOC_S_FMT, &f) < 0) {
    fprintf(stderr, "VIDIOC_S_FMT: %s\n", strerror(errno));
    return 1;
  }
  struct v4l2_streamparm p;
  memset(&p, 0, sizeof p);
  p.type = V4L2_BUF_TYPE_VIDEO_OUTPUT;
  p.parm.output.timeperframe.numerator = 1;
  p.parm.output.timeperframe.denominator = FPS;
  ioctl(fd, VIDIOC_S_PARM, &p); // not every version takes it

  uint8_t *in = malloc(NV12), *out = malloc(YUYV);
  if (!in || !out) return 1;
  black(out);
  size_t got = 0;
  int64_t last = 0, next = now_ms();
  fcntl(0, F_SETFL, fcntl(0, F_GETFL) | O_NONBLOCK);
  for (;;) {
    // A frame out when it's time, whatever is coming in.
    int64_t t = now_ms();
    if (t >= next) {
      next += 1000 / FPS;
      if (t - next > 1000) next = t;
      if (last && t - last > STALE_MS) {
        black(out);
        last = 0;
      }
      if (write(fd, out, YUYV) < 0 && errno != EAGAIN) {
        fprintf(stderr, "write: %s\n", strerror(errno));
        return 1;
      }
      continue;
    }
    struct pollfd pf = { .fd = 0, .events = POLLIN };
    if (poll(&pf, 1, (int)(next - t)) <= 0) continue;
    ssize_t n = read(0, in + got, NV12 - got);
    if (n == 0) break; // TwinDesk stopped: the camera goes with it
    if (n > 0 && (got += (size_t)n) == NV12) {
      to_yuyv(in, out);
      got = 0;
      last = now_ms();
    }
  }
  return 0;
}
