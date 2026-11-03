// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The phone's sound on this PC as its Bluetooth speaker (Audio › Play phone sound here
// (Bluetooth), windows/btaudio.js): Windows' AudioPlaybackConnection (Windows 10 2004 and later)
// takes a paired phone's A2DP audio and plays it on the default output, and the phone sends its
// sound there as to any speaker. The phone has to be paired with this PC over classic Bluetooth.
//   a2dpList -> {devices: [{id, name}]}: paired devices that can send sound
//   a2dpOpen {id} -> {}: on (or Windows' reason why not); a2dpClose
//   event a2dp.state {state: 'opened' | 'closed'}
using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Windows.Devices.Enumeration;
using Windows.Media.Audio;

namespace MacShare
{
    static class A2dp
    {
        static readonly object gate = new object();
        static AudioPlaybackConnection conn;

        public static async Task<Dictionary<string, object>> List()
        {
            List<object> list = new List<object>();
            foreach (DeviceInformation d in await DeviceInformation.FindAllAsync(AudioPlaybackConnection.GetDeviceSelector()))
            {
                Dictionary<string, object> x = new Dictionary<string, object>();
                x["id"] = d.Id;
                x["name"] = d.Name;
                list.Add(x);
            }
            Dictionary<string, object> r = new Dictionary<string, object>();
            r["devices"] = list;
            return r;
        }

        public static async Task<Dictionary<string, object>> Open(string id)
        {
            Close();
            AudioPlaybackConnection c = AudioPlaybackConnection.TryCreateFromId(id);
            if (c == null) throw new Exception("that device doesn't send sound");
            c.StateChanged += delegate (AudioPlaybackConnection s, object a)
            {
                Net.Event("a2dp.state", null, "state", s.State == AudioPlaybackConnectionState.Opened ? "opened" : "closed");
            };
            await c.StartAsync();
            AudioPlaybackConnectionOpenResult r = await c.OpenAsync();
            if (r.Status != AudioPlaybackConnectionOpenResultStatus.Success)
            {
                c.Dispose();
                string why = r.Status == AudioPlaybackConnectionOpenResultStatus.RequestTimedOut ? "the phone didn't answer"
                    : r.Status == AudioPlaybackConnectionOpenResultStatus.DeniedBySystem ? "Windows refused it"
                    : "it didn't work" + (r.ExtendedError != null ? " (" + r.ExtendedError.Message + ")" : "");
                throw new Exception(why);
            }
            lock (gate) conn = c;
            Net.Event("a2dp.state", null, "state", "opened");
            return null;
        }

        public static void Close()
        {
            AudioPlaybackConnection c;
            lock (gate) { c = conn; conn = null; }
            if (c == null) return;
            try { c.Dispose(); } catch (Exception) { }
            Net.Event("a2dp.state", null, "state", "closed");
        }
    }
}
