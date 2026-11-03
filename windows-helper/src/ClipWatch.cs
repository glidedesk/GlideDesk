// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// A "clipboard" event whenever anything is copied (WM_CLIPBOARDUPDATE on a message-only window),
// pictures included: windows/clip.js then looks at the clipboard once instead of reading a picture
// every time it polls. {seq}: Windows' clipboard sequence number.
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Windows.Forms;

namespace MacShare
{
    class ClipWatch : NativeWindow
    {
        const int WM_CLIPBOARDUPDATE = 0x031D;
        static readonly IntPtr HWND_MESSAGE = new IntPtr(-3);

        [DllImport("user32.dll", SetLastError = true)] static extern bool AddClipboardFormatListener(IntPtr hwnd);
        [DllImport("user32.dll", SetLastError = true)] static extern bool RemoveClipboardFormatListener(IntPtr hwnd);
        [DllImport("user32.dll")] static extern uint GetClipboardSequenceNumber();

        readonly Action<Dictionary<string, object>> emit;

        // On the UI thread (it has the message loop).
        public ClipWatch(Action<Dictionary<string, object>> emit)
        {
            this.emit = emit;
            CreateParams cp = new CreateParams();
            cp.Parent = HWND_MESSAGE;
            CreateHandle(cp);
            AddClipboardFormatListener(Handle);
        }

        public void Stop()
        {
            if (Handle == IntPtr.Zero) return;
            RemoveClipboardFormatListener(Handle);
            DestroyHandle();
        }

        protected override void WndProc(ref Message m)
        {
            if (m.Msg == WM_CLIPBOARDUPDATE)
            {
                Dictionary<string, object> e = new Dictionary<string, object>();
                e["event"] = "clipboard";
                e["seq"] = (double)GetClipboardSequenceNumber();
                emit(e);
            }
            base.WndProc(ref m);
        }
    }
}
