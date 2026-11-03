// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// A phone's stylus (the `pen` command: in.* with `pen`, shared/protocol.js P.penOf) as a tablet
// pen, on X11 and Wayland alike: a uinput tablet (/dev/uinput) with its pressure and tilt, the side
// button as BTN_STYLUS and the eraser end as the rubber tool, which libinput hands the desktop as a
// pen (drawing apps get its pressure). Its area is the whole desktop (screens_box), as the
// compositor maps a tablet that isn't a screen of its own. Without /dev/uinput, the pen is a mouse.
#include "helper.h"
#include <fcntl.h>
#include <linux/uinput.h>
#include <math.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define PEN_MAXV 65535
#define PRESSURE_MAX 4095

static int fd = -1, tried;
static double bornAt; // a new device: the desktop opens it a moment later, and would miss what comes before
static int near, touching, barrel, rubber, mouseDown;

static void pemit(int type, int code, int value) {
  struct input_event ie;
  memset(&ie, 0, sizeof ie);
  ie.type = (unsigned short)type;
  ie.code = (unsigned short)code;
  ie.value = value;
  if (write(fd, &ie, sizeof ie) < 0) { /* full: dropped */ }
}

static void abs_setup(int code, int min, int max, int res) {
  struct uinput_abs_setup ab;
  memset(&ab, 0, sizeof ab);
  ab.code = (unsigned short)code;
  ab.absinfo.minimum = min;
  ab.absinfo.maximum = max;
  ab.absinfo.resolution = res;
  ioctl(fd, UI_ABS_SETUP, &ab);
}

static int ready(void) {
  if (tried) return fd >= 0;
  tried = 1;
  fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK | O_CLOEXEC);
  if (fd < 0) { fprintf(stderr, "pen: no /dev/uinput, the stylus is a mouse\n"); return 0; }
  ioctl(fd, UI_SET_EVBIT, EV_KEY);
  ioctl(fd, UI_SET_EVBIT, EV_ABS);
  ioctl(fd, UI_SET_EVBIT, EV_SYN);
  ioctl(fd, UI_SET_KEYBIT, BTN_TOOL_PEN);
  ioctl(fd, UI_SET_KEYBIT, BTN_TOOL_RUBBER);
  ioctl(fd, UI_SET_KEYBIT, BTN_TOUCH);
  ioctl(fd, UI_SET_KEYBIT, BTN_STYLUS);
  int axes[] = { ABS_X, ABS_Y, ABS_PRESSURE, ABS_TILT_X, ABS_TILT_Y };
  for (unsigned i = 0; i < sizeof axes / sizeof *axes; i++) ioctl(fd, UI_SET_ABSBIT, axes[i]);
  ioctl(fd, UI_SET_PROPBIT, INPUT_PROP_POINTER);
  struct uinput_setup us;
  memset(&us, 0, sizeof us);
  us.id.bustype = BUS_VIRTUAL;
  us.id.vendor = 0x1209;
  us.id.product = 0x7d03;
  snprintf(us.name, sizeof us.name, "TwinDesk virtual pen"); // "TwinDesk virtual": not read back (wl.c)
  if (ioctl(fd, UI_DEV_SETUP, &us) < 0) goto fail;
  abs_setup(ABS_X, 0, PEN_MAXV, 200); // libinput needs a size: about 33 cm wide
  abs_setup(ABS_Y, 0, PEN_MAXV, 200);
  abs_setup(ABS_PRESSURE, 0, PRESSURE_MAX, 0);
  abs_setup(ABS_TILT_X, -90, 90, 57); // degrees (units per radian)
  abs_setup(ABS_TILT_Y, -90, 90, 57);
  if (ioctl(fd, UI_DEV_CREATE) < 0) goto fail;
  bornAt = now_ms();
  return 1;
fail:
  fprintf(stderr, "pen: no uinput tablet, the stylus is a mouse\n");
  close(fd);
  fd = -1;
  return 0;
}

static void mouse(int wl, int x, int y, int out, int c) {
  if (!out) { if (wl) wl_move_to(x, y); else inject_move_to(x, y); }
  if (c != mouseDown) {
    mouseDown = c;
    if (wl) wl_button(0, c); else inject_button(0, c);
  }
}

// Out of range: lifted first if it still touches (a session ended, releaseAll).
void pen_leave(int wl) {
  if (mouseDown) mouse(wl, 0, 0, 1, 0);
  if (fd < 0 || !near) return;
  if (touching) { pemit(EV_ABS, ABS_PRESSURE, 0); pemit(EV_KEY, BTN_TOUCH, 0); touching = 0; }
  if (barrel) { pemit(EV_KEY, BTN_STYLUS, 0); barrel = 0; }
  pemit(EV_KEY, rubber ? BTN_TOOL_RUBBER : BTN_TOOL_PEN, 0);
  pemit(EV_SYN, SYN_REPORT, 0);
  near = 0;
}

// The stylus at x,y (desktop px, as moveTo), in its state `pen`.
void pen_at(int wl, int x, int y, const JObj *pen) {
  int out = jbool(pen, "out", 0);
  int c = jbool(pen, "c", 0) && !out;
  if (!ready()) { mouse(wl, x, y, out, c); return; }
  if (out) { pen_leave(wl); return; }
  if (now_ms() - bornAt < 300) return; // its first frame says the pen is near once someone listens
  int er = jbool(pen, "er", 0);
  if (near && er != rubber) pen_leave(wl); // the other end
  int bx, by, bw, bh;
  screens_box(&bx, &by, &bw, &bh);
  int ax = (int)lround((x - bx) * (double)PEN_MAXV / (bw > 1 ? bw - 1 : 1));
  int ay = (int)lround((y - by) * (double)PEN_MAXV / (bh > 1 ? bh - 1 : 1));
  pemit(EV_ABS, ABS_X, ax < 0 ? 0 : ax > PEN_MAXV ? PEN_MAXV : ax);
  pemit(EV_ABS, ABS_Y, ay < 0 ? 0 : ay > PEN_MAXV ? PEN_MAXV : ay);
  pemit(EV_ABS, ABS_TILT_X, (int)lround(fmax(-90, fmin(90, jnum(pen, "tx", 0)))));
  pemit(EV_ABS, ABS_TILT_Y, (int)lround(fmax(-90, fmin(90, jnum(pen, "ty", 0)))));
  pemit(EV_ABS, ABS_PRESSURE, c ? (int)lround(fmax(0, fmin(1, jnum(pen, "p", 0))) * PRESSURE_MAX) : 0);
  if (!near) { rubber = er; pemit(EV_KEY, rubber ? BTN_TOOL_RUBBER : BTN_TOOL_PEN, 1); near = 1; }
  if (c != touching) { touching = c; pemit(EV_KEY, BTN_TOUCH, c); }
  int bar = jbool(pen, "bar", 0);
  if (bar != barrel) { barrel = bar; pemit(EV_KEY, BTN_STYLUS, bar); }
  pemit(EV_SYN, SYN_REPORT, 0);
  if (wl) wl_pen_moved(x, y); // where the pointer is now, for the edges (wl.c follows it)
}
