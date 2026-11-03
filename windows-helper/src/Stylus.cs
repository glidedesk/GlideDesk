// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// A phone's stylus (the `pen` command: in.* with `pen`, shared/protocol.js P.penOf) as a Windows
// pen: a synthetic pen pointer device (InjectSyntheticPointerInput, Windows 10 1809 and later), with
// its pressure, tilt and rotation, the side button as the barrel button and the eraser end as the
// eraser, so ink apps see a pen and others get the mouse Windows makes of it. Where pointer
// injection isn't there, the pen is a mouse (MoveTo and the left button).
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace MacShare
{
    class Stylus
    {
        const uint PT_PEN = 3, POINTER_FEEDBACK_DEFAULT = 1;
        const uint POINTER_FLAG_INRANGE = 0x2, POINTER_FLAG_INCONTACT = 0x4, POINTER_FLAG_FIRSTBUTTON = 0x10,
            POINTER_FLAG_PRIMARY = 0x2000, POINTER_FLAG_DOWN = 0x10000, POINTER_FLAG_UPDATE = 0x20000, POINTER_FLAG_UP = 0x40000;
        const uint PEN_FLAG_BARREL = 0x1, PEN_FLAG_INVERTED = 0x2, PEN_FLAG_ERASER = 0x4;
        const uint PEN_MASK_PRESSURE = 0x1, PEN_MASK_ROTATION = 0x2, PEN_MASK_TILT_X = 0x4, PEN_MASK_TILT_Y = 0x8;
        const int POINTER_CHANGE_NONE = 0, POINTER_CHANGE_FIRSTBUTTON_DOWN = 1, POINTER_CHANGE_FIRSTBUTTON_UP = 2;

        [StructLayout(LayoutKind.Sequential)]
        struct POINTER_INFO
        {
            public uint pointerType, pointerId, frameId, pointerFlags;
            public IntPtr sourceDevice, hwndTarget;
            public Native.POINT ptPixelLocation, ptHimetricLocation, ptPixelLocationRaw, ptHimetricLocationRaw;
            public uint dwTime, historyCount;
            public int InputData;
            public uint dwKeyStates;
            public ulong PerformanceCount;
            public int ButtonChangeType;
        }

        [StructLayout(LayoutKind.Sequential)]
        struct POINTER_PEN_INFO
        {
            public POINTER_INFO pointerInfo;
            public uint penFlags, penMask, pressure, rotation;
            public int tiltX, tiltY;
        }

        // POINTER_TYPE_INFO: the type, then a union of the touch and pen infos (the touch one is
        // 24 bytes longer: the padding).
        [StructLayout(LayoutKind.Sequential)]
        struct POINTER_TYPE_INFO
        {
            public uint type;
            public POINTER_PEN_INFO pen;
            public ulong pad0, pad1, pad2;
        }

        [DllImport("user32.dll", SetLastError = true)]
        static extern IntPtr CreateSyntheticPointerDevice(uint pointerType, uint maxCount, uint mode);

        [DllImport("user32.dll", SetLastError = true)]
        static extern bool InjectSyntheticPointerInput(IntPtr device, [In] POINTER_TYPE_INFO[] info, uint count);

        readonly Injector injector;
        IntPtr device = IntPtr.Zero;
        bool tried, inRange, contact, mouseDown;
        int lastX, lastY;

        public Stylus(Injector injector)
        {
            this.injector = injector;
        }

        bool Ready()
        {
            if (!tried)
            {
                tried = true;
                try { device = CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_DEFAULT); }
                catch (EntryPointNotFoundException) { device = IntPtr.Zero; } // before Windows 10 1809
                if (device == IntPtr.Zero) Console.Error.WriteLine("pen: no synthetic pointer device (" + Marshal.GetLastWin32Error() + "), the stylus is a mouse");
            }
            return device != IntPtr.Zero;
        }

        // The stylus at x,y (physical px on the virtual desktop), in its state `pen`.
        public void At(int x, int y, Dictionary<string, object> pen)
        {
            bool out_ = Json.Bool(pen, "out", false);
            bool c = Json.Bool(pen, "c", false) && !out_;
            if (!Ready())
            {
                if (!out_) Injector.MoveTo(x, y);
                if (c != mouseDown) { mouseDown = c; injector.Button(0, c); }
                return;
            }
            lastX = x;
            lastY = y;
            if (out_)
            {
                Leave();
                return;
            }
            if (!inRange) { Send(POINTER_FLAG_INRANGE | POINTER_FLAG_UPDATE, pen, POINTER_CHANGE_NONE); inRange = true; }
            if (c && !contact)
            {
                contact = true;
                Send(POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_FIRSTBUTTON | POINTER_FLAG_DOWN, pen, POINTER_CHANGE_FIRSTBUTTON_DOWN);
            }
            else if (!c && contact)
            {
                contact = false;
                Send(POINTER_FLAG_INRANGE | POINTER_FLAG_UP, pen, POINTER_CHANGE_FIRSTBUTTON_UP);
            }
            else Send(POINTER_FLAG_INRANGE | POINTER_FLAG_UPDATE | (c ? POINTER_FLAG_INCONTACT | POINTER_FLAG_FIRSTBUTTON : 0), pen, POINTER_CHANGE_NONE);
        }

        // Out of range: lifted first if it still touches (a session ended, releaseAll).
        public void Leave()
        {
            if (mouseDown) { mouseDown = false; injector.Button(0, false); }
            if (device == IntPtr.Zero) return;
            Dictionary<string, object> none = new Dictionary<string, object>();
            if (contact) { contact = false; Send(POINTER_FLAG_INRANGE | POINTER_FLAG_UP, none, POINTER_CHANGE_FIRSTBUTTON_UP); }
            if (inRange) { inRange = false; Send(POINTER_FLAG_UPDATE, none, POINTER_CHANGE_NONE); }
        }

        void Send(uint flags, Dictionary<string, object> pen, int change)
        {
            POINTER_TYPE_INFO[] info = new POINTER_TYPE_INFO[1];
            info[0].type = PT_PEN;
            info[0].pen.pointerInfo.pointerType = PT_PEN;
            info[0].pen.pointerInfo.pointerFlags = flags | POINTER_FLAG_PRIMARY;
            info[0].pen.pointerInfo.ptPixelLocation = new Native.POINT(lastX, lastY);
            info[0].pen.pointerInfo.ButtonChangeType = change;
            bool er = Json.Bool(pen, "er", false);
            uint penFlags = 0;
            if (Json.Bool(pen, "bar", false)) penFlags |= PEN_FLAG_BARREL;
            if (er) penFlags |= PEN_FLAG_INVERTED | ((flags & POINTER_FLAG_INCONTACT) != 0 ? PEN_FLAG_ERASER : 0);
            info[0].pen.penFlags = penFlags;
            info[0].pen.penMask = PEN_MASK_PRESSURE | PEN_MASK_ROTATION | PEN_MASK_TILT_X | PEN_MASK_TILT_Y;
            double p = (flags & POINTER_FLAG_INCONTACT) != 0 ? Json.Num(pen, "p", 0) : 0;
            info[0].pen.pressure = (uint)Math.Round(Math.Min(1, Math.Max(0, p)) * 1024);
            info[0].pen.rotation = (uint)(((int)Math.Round(Json.Num(pen, "tw", 0)) % 360 + 360) % 360);
            info[0].pen.tiltX = (int)Math.Round(Math.Min(90, Math.Max(-90, Json.Num(pen, "tx", 0))));
            info[0].pen.tiltY = (int)Math.Round(Math.Min(90, Math.Max(-90, Json.Num(pen, "ty", 0))));
            if (!InjectSyntheticPointerInput(device, info, 1)) Console.Error.WriteLine("pen: inject failed (" + Marshal.GetLastWin32Error() + ")");
            Injector.Wake();
        }
    }
}
