// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Multi-device tab: native multi-device mesh status and settings.
'use strict';

UI.tab({
  id: 'multi',
  order: 95,
  title: 'Multi-device',
  color: 'linear-gradient(#5e5ce6, #3634a3)',
  desc: 'Several computers and phones connected at once',
  bottom: true,
  build(body) {
    const statusPill = UI.el('span', { class: 'pill good', text: 'Enabled' });
    const meshRow = UI.row('Multi-Device Mesh Active',
      'GlideDesk operates natively as a multi-device mesh. All connected computers and phones share keyboard, mouse, and clipboard simultaneously across screen edges without needing mode switches.',
      [statusPill]
    );

    body.append(
      UI.card('Mesh Networking', [meshRow]),
      UI.note('info', 'Connect to other computers or phones using "Connect by address" or "Found nearby" on the Overview page.'),
      MeshIntro.art(),
    );

    return () => {};
  },
});
