# Diamond Stúdió távsegítség – módosítási nyilatkozat / Modification notice

Ez a program a [RustDesk](https://github.com/rustdesk/rustdesk) 1.5.0 módosított változata.
This program is a modified version of RustDesk 1.5.0.

- Eredeti mű / Original work: Copyright © Purslane Tech Pte. Ltd. and RustDesk contributors
- Módosította / Modified by: Diamond Stúdió, 2026-10-31 – 2026-11-01
- Licenc / License: GNU Affero General Public License v3.0 – lásd / see [LICENCE](LICENCE)
- Forráskód / Source code: https://github.com/zoli1969/rustdesk/tree/diamond
  (a kiadott változatok címkéi / release tags: `diamond-*`)

A program GARANCIA NÉLKÜL érhető el. / This program comes WITHOUT ANY WARRANTY.

## Módosítások / Changes

| Fájl / File | Módosítás / Change |
|---|---|
| `src/common.rs` | Beépített programnév és szerver (`rustdesk.diamonduzenet.hu`); a szerverkulcs fordításkor a `DIAMOND_RS_KEY` környezeti változóból kerül be; a hang alapértelmezetten kikapcsolva / Built-in app name and server; the server key is injected at compile time from the `DIAMOND_RS_KEY` environment variable; audio off by default |
| `src/common.rs` | Az ügyfeleknek kiadott változat csak fogadni tud kapcsolatot (`conn-type` = `incoming`); a belső, teljes változat (`DiamondVezerlo`, „Diamond Stúdió vezérlő”, saját belső név és kibontási mappa) a `DIAMOND_FULL_CLIENT=Y` fordítási változóval készül / The customer build is incoming-only; the internal full build is made with `DIAMOND_FULL_CLIENT=Y` |
| `src/lang.rs`, `src/ui.rs`, `src/tray.rs`, `src/platform/windows.rs`, `flutter/lib/common.dart`, `src/ui/index.tis` | A belső programnév ékezet nélküli (`DiamondTavsegites`, a telepítő csak `[a-zA-Z0-9-]+` nevet fogad el); az ablakcím, a feliratok, a tálcaikon és a Programok listája a „Diamond Stúdió távsegítség” nevet mutatja / ASCII internal app name for the installer; titles, texts, tray and Programs list show the display name |
| `res/*`, `flutter/assets/*`, `flutter/windows/runner/resources/app_icon.ico`, `src/ui.rs` | Diamond Stúdió ikonok és logó / Diamond Stúdió icons and logo |
| `Cargo.toml`, `libs/portable/Cargo.toml`, `flutter/windows/runner/Runner.rc` | Windows verzióadatok (terméknév, leírás) / Windows version info |
| `flutter/lib/desktop/pages/desktop_setting_page.dart`, `src/ui/index.tis` | Licenc- és forráskód-tájékoztatás a Névjegyben / License and source notice in the About page |
| `.github/workflows/diamond-windows.yml` | Windows x64 (Flutter) és x86 (Sciter) build / Windows build workflow |

## Harmadik fél összetevői / Third-party components

A 32 bites (x86) változat a Sciter motort használja (https://sciter.com),
© Terra Informatica Software, Inc., a saját licence szerint.
The 32-bit (x86) build uses the Sciter engine under its own license.

A „RustDesk” név és logó a Purslane Tech Pte. Ltd. védjegye lehet; ez a változat nem a hivatalos RustDesk.
"RustDesk" may be a trademark of Purslane Tech Pte. Ltd.; this build is not the official RustDesk.
