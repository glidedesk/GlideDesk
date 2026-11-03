// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// ReZume (windows/activity.js): what is in use in the window in front. Its app (the .exe), its
// title, and what it shows: a browser's address (its address bar, read with UI Automation, never the
// page itself), else the document whose name the title shows, found among the files Windows lists
// as recently opened (Recent's shortcuts). Asked only while ReZume is on; the address is read again
// only when the window or its title changed.
using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Windows.Automation;

namespace MacShare
{
    static class Activity
    {
        [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
        [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
        [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder name, int max);
        [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr h, int flags, StringBuilder name, ref int size);

        static readonly Regex Browsers = new Regex(@"^(chrome|msedge|firefox|brave|vivaldi|opera|chromium|librewolf|waterfox|zen|arc)$", RegexOptions.IgnoreCase);
        static readonly Regex Hostish = new Regex(@"^([\w-]+\.)+[a-z]{2,}(:\d+)?([/?#]|$)|^localhost(:\d+)?([/?#]|$)", RegexOptions.IgnoreCase);
        static long lastHwnd;
        static string lastTitle = "";
        static string lastUrl = "";
        static readonly Dictionary<string, string> links = new Dictionary<string, string>(); // a Recent shortcut (path|time) → its target

        public static void Front(Dictionary<string, object> r)
        {
            IntPtr root = GetAncestor(GetForegroundWindow(), 2); // GA_ROOT
            if (root == IntPtr.Zero || !IsWindowVisible(root)) return;
            StringBuilder cls = new StringBuilder(64);
            GetClassName(root, cls, cls.Capacity);
            string c = cls.ToString();
            if (c == "Progman" || c == "WorkerW" || c == "Shell_TrayWnd" || c == "Shell_SecondaryTrayWnd" || c == "CabinetWClass" || c == "ExploreWClass") return;
            uint pid;
            GetWindowThreadProcessId(root, out pid);
            StringBuilder t = new StringBuilder(512);
            GetWindowText(root, t, t.Capacity);
            string title = t.ToString();
            string exe = ExeOf(pid);
            r["hwnd"] = (double)root.ToInt64();
            r["pid"] = (double)pid;
            r["exe"] = exe;
            r["title"] = title;
            string name = Path.GetFileNameWithoutExtension(exe);
            if (Browsers.IsMatch(name))
            {
                if (root.ToInt64() != lastHwnd || title != lastTitle)
                {
                    lastHwnd = root.ToInt64();
                    lastTitle = title;
                    lastUrl = "";
                    try { lastUrl = AddressOf(root); } catch (Exception) { lastUrl = ""; }
                }
                r["browser"] = true;
                if (lastUrl != "") r["url"] = lastUrl;
                return;
            }
            string file = null;
            try { file = RecentFile(title); } catch (Exception) { file = null; }
            if (file != null) r["file"] = file;
        }

        static string ExeOf(uint pid)
        {
            IntPtr h = OpenProcess(0x1000, false, pid); // PROCESS_QUERY_LIMITED_INFORMATION
            if (h == IntPtr.Zero) return "";
            try
            {
                StringBuilder sb = new StringBuilder(1024);
                int size = sb.Capacity;
                return QueryFullProcessImageName(h, 0, sb, ref size) ? sb.ToString() : "";
            }
            finally { CloseHandle(h); }
        }

        // The address bar: the first edit box in the window's own controls (pages are documents,
        // never gone into), Firefox's by its id. Not while it's being typed in.
        static string AddressOf(IntPtr hwnd)
        {
            AutomationElement root = AutomationElement.FromHandle(hwnd);
            TreeWalker w = TreeWalker.ControlViewWalker;
            Queue<KeyValuePair<AutomationElement, int>> q = new Queue<KeyValuePair<AutomationElement, int>>();
            q.Enqueue(new KeyValuePair<AutomationElement, int>(root, 0));
            int seen = 0;
            string first = "";
            while (q.Count > 0 && seen++ < 600)
            {
                KeyValuePair<AutomationElement, int> e = q.Dequeue();
                AutomationElement el = e.Key;
                ControlType ct;
                string id;
                try { ct = el.Current.ControlType; id = el.Current.AutomationId ?? ""; } catch (Exception) { continue; }
                if (ct == ControlType.Document) continue;
                if (ct == ControlType.Edit || id == "urlbar-input")
                {
                    bool typing = false;
                    try { typing = el.Current.HasKeyboardFocus; } catch (Exception) { }
                    string v = typing ? "" : ValueOf(el);
                    string u = Normalize(v);
                    if (u != "")
                    {
                        if (id == "urlbar-input") return u;
                        if (first == "") first = u;
                        return first;
                    }
                    continue;
                }
                if (e.Value >= 14) continue;
                AutomationElement child = null;
                try { child = w.GetFirstChild(el); } catch (Exception) { }
                while (child != null)
                {
                    q.Enqueue(new KeyValuePair<AutomationElement, int>(child, e.Value + 1));
                    try { child = w.GetNextSibling(child); } catch (Exception) { child = null; }
                }
            }
            return first;
        }

        static string ValueOf(AutomationElement el)
        {
            object p;
            if (el.TryGetCurrentPattern(ValuePattern.Pattern, out p)) return ((ValuePattern)p).Current.Value ?? "";
            return "";
        }

        // Chromium shows "example.com/page" while not edited: that's https://example.com/page.
        static string Normalize(string v)
        {
            v = (v ?? "").Trim();
            if (v == "" || v.IndexOf(' ') >= 0) return "";
            if (v.StartsWith("http://", StringComparison.OrdinalIgnoreCase) || v.StartsWith("https://", StringComparison.OrdinalIgnoreCase)) return v;
            return Hostish.IsMatch(v) ? "https://" + v : "";
        }

        // The file of Recent's newest shortcuts whose name the window's title shows ("report.docx -
        // Word", "report - Word", "notes.txt - Notepad").
        static string RecentFile(string title)
        {
            if (title.Length < 3) return null;
            string dir = Environment.GetFolderPath(Environment.SpecialFolder.Recent);
            if (!Directory.Exists(dir)) return null;
            FileInfo[] lnks = new DirectoryInfo(dir).GetFiles("*.lnk");
            Array.Sort(lnks, delegate (FileInfo a, FileInfo b) { return b.LastWriteTimeUtc.CompareTo(a.LastWriteTimeUtc); });
            int n = 0;
            foreach (FileInfo l in lnks)
            {
                if (n++ >= 80) break;
                string name = Path.GetFileNameWithoutExtension(l.Name); // "report.docx"
                string stem = Path.GetFileNameWithoutExtension(name);
                if (stem == name || stem.Length < 2) continue; // a folder's shortcut
                if (!Shows(title, name) && !Shows(title, stem)) continue;
                string target = Target(l);
                if (!string.IsNullOrEmpty(target) && File.Exists(target)) return target;
            }
            return null;
        }

        static bool Shows(string title, string name)
        {
            int i = title.IndexOf(name, StringComparison.OrdinalIgnoreCase);
            if (i < 0) return false;
            bool start = i == 0 || " *[(-—–•|".IndexOf(title[i - 1]) >= 0;
            int end = i + name.Length;
            bool stop = end == title.Length || " *])-—–•|:".IndexOf(title[end]) >= 0;
            return start && stop;
        }

        static string Target(FileInfo lnk)
        {
            string key = lnk.FullName + "|" + lnk.LastWriteTimeUtc.Ticks;
            string t;
            if (links.TryGetValue(key, out t)) return t;
            Type type = Type.GetTypeFromProgID("WScript.Shell");
            object sh = Activator.CreateInstance(type);
            try
            {
                object sc = type.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, sh, new object[] { lnk.FullName });
                t = Convert.ToString(sc.GetType().InvokeMember("TargetPath", BindingFlags.GetProperty, null, sc, null));
                Marshal.ReleaseComObject(sc);
            }
            finally { Marshal.ReleaseComObject(sh); }
            if (links.Count > 500) links.Clear();
            links[key] = t;
            return t;
        }
    }
}
