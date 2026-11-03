// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Input methods across computers (shared/keylayout.js): keys typed with one (Japanese, Pinyin, Korean)
// type a–z and the input method composes them, so the language goes along with the keys (keyLayout's
// im) and the other side switches to its own input method of that language while driven (selectInput),
// back to what it had after. Windows' CJK input methods go with their language's keyboard layout (HKL).
using System;
using System.Runtime.InteropServices;
using System.Threading;

namespace MacShare
{
    static class InputMethods
    {
        [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hWnd, IntPtr processId);
        [DllImport("user32.dll")] static extern IntPtr GetKeyboardLayout(uint threadId);
        [DllImport("user32.dll")] static extern int GetKeyboardLayoutList(int n, [Out] IntPtr[] list);
        [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out IntPtr result);
        [DllImport("imm32.dll")] static extern IntPtr ImmGetDefaultIMEWnd(IntPtr hWnd);

        const uint WM_INPUTLANGCHANGEREQUEST = 0x0050, WM_IME_CONTROL = 0x0283, SMTO_ABORTIFHUNG = 0x0002;
        const int IMC_GETCONVERSIONMODE = 0x1, IMC_SETCONVERSIONMODE = 0x2, IMC_GETOPENSTATUS = 0x5, IMC_SETOPENSTATUS = 0x6;
        const int IME_CMODE_NATIVE = 0x1;

        static IntPtr before = IntPtr.Zero; // the layout in use before Select, for Restore

        // An input method language by LANGID: Chinese simplified (PRC, Singapore) and traditional
        // (Taiwan, Hong Kong, Macao), Japanese, Korean.
        static string Lang(IntPtr hkl)
        {
            int id = (int)((long)hkl & 0xFFFF);
            switch (id)
            {
                case 0x0804: case 0x1004: return "zh-Hans";
                case 0x0404: case 0x0C04: case 0x1404: return "zh-Hant";
                case 0x0411: return "ja";
                case 0x0412: return "ko";
            }
            return null;
        }

        static IntPtr ImeWnd(IntPtr w) { return w == IntPtr.Zero ? IntPtr.Zero : ImmGetDefaultIMEWnd(w); }

        static int Control(IntPtr ime, int cmd, int arg)
        {
            IntPtr r;
            if (ime == IntPtr.Zero || SendMessageTimeout(ime, WM_IME_CONTROL, (IntPtr)cmd, (IntPtr)arg, SMTO_ABORTIFHUNG, 200, out r) == IntPtr.Zero) return -1;
            return (int)(long)r;
        }

        // keyLayout's im: the language of the input method typed with in the window in front, while it
        // composes (open, in its own script: not Japanese's direct input or Pinyin's English mode); or null.
        public static string Current()
        {
            IntPtr w = Native.GetForegroundWindow();
            string lang = Lang(GetKeyboardLayout(GetWindowThreadProcessId(w, IntPtr.Zero)));
            if (lang == null) return null;
            IntPtr ime = ImeWnd(w);
            int open = Control(ime, IMC_GETOPENSTATUS, 0), mode = Control(ime, IMC_GETCONVERSIONMODE, 0);
            if (open == 0 || (mode >= 0 && (mode & IME_CMODE_NATIVE) == 0)) return null;
            return lang;
        }

        // An installed input method of that language, composing; false when none is installed.
        public static bool Select(string lang)
        {
            IntPtr w = Native.GetForegroundWindow();
            IntPtr cur = GetKeyboardLayout(GetWindowThreadProcessId(w, IntPtr.Zero));
            IntPtr to = IntPtr.Zero;
            if (Lang(cur) == lang) to = cur;
            else
            {
                IntPtr[] list = new IntPtr[64];
                int n = GetKeyboardLayoutList(list.Length, list);
                for (int i = 0; i < n && to == IntPtr.Zero; i++) if (Lang(list[i]) == lang) to = list[i];
                if (to == IntPtr.Zero) return false;
                if (before == IntPtr.Zero) before = cur;
                Native.PostMessage(w, WM_INPUTLANGCHANGEREQUEST, IntPtr.Zero, to);
                Thread.Sleep(150); // the window takes it in its own time
            }
            // Japanese starts in direct input, Korean in English, Pinyin may be in its English mode.
            IntPtr ime = ImeWnd(w);
            Control(ime, IMC_SETOPENSTATUS, 1);
            int mode = Control(ime, IMC_GETCONVERSIONMODE, 0);
            if (mode >= 0 && (mode & IME_CMODE_NATIVE) == 0) Control(ime, IMC_SETCONVERSIONMODE, mode | IME_CMODE_NATIVE);
            try { Console.Error.WriteLine("input method: " + lang); } catch (Exception) { }
            return true;
        }

        // Back to the layout in use before Select.
        public static void Restore()
        {
            if (before == IntPtr.Zero) return;
            Native.PostMessage(Native.GetForegroundWindow(), WM_INPUTLANGCHANGEREQUEST, IntPtr.Zero, before);
            before = IntPtr.Zero;
        }
    }
}
