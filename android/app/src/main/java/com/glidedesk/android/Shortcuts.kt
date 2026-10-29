// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// The connected computer in Android's share sheet: a sharing shortcut ("Desktop") in the
// top row while it is connected, so sharing to it is one tap from any app (ShareActivity sends).
package com.glidedesk.android

import android.content.Intent
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat

class Shortcuts(core: Core) : Feature(core) {
    private val category = "com.glidedesk.android.SHARE_TARGET"

    override fun onWelcome(host: Host) {
        runCatching {
            val s = ShortcutInfoCompat.Builder(core.ctx, "send-${host.id}")
                .setShortLabel(host.name)
                .setLongLabel("Send to ${host.name}")
                .setIcon(IconCompat.createWithResource(core.ctx, R.mipmap.ic_launcher))
                .setCategories(setOf(category))
                .setLongLived(true)
                .setIntent(Intent(core.ctx, MainActivity::class.java).setAction(Intent.ACTION_VIEW))
                .build()
            ShortcutManagerCompat.removeAllDynamicShortcuts(core.ctx)
            ShortcutManagerCompat.pushDynamicShortcut(core.ctx, s)
        }
    }

    override fun onClosed() {
        runCatching { ShortcutManagerCompat.removeAllDynamicShortcuts(core.ctx) }
    }
}
