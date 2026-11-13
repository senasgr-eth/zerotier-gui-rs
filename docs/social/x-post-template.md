# Template posting X / Twitter

Template untuk mempromosikan **ZeroTier-GUI (Rust + GPUI)**. Ada versi singkat,
thread, dan catatan gambar/alt text.

Repo: <https://github.com/senasgr-eth/zerotier-gui-rs>
Release: <https://github.com/senasgr-eth/zerotier-gui-rs/releases>

---

## 1. Post utama — English

```text
I rewrote ZeroTier-GUI in Rust + GPUI 🦀

The original Python app was dormant for years, so I rebuilt it natively:
• networks, peers & paths
• join/leave + history
• Linux, macOS & Windows builds
• GPL-3.0, original by Tomás Ralph

github.com/senasgr-eth/zerotier-gui-rs
```

**Gambar:** `img/managenetworks1.png` + `img/managepeers.png`
**Alt text gambar 1:** ZeroTier-GUI app window showing joined networks with their IDs, names and status.
**Alt text gambar 2:** Peers screen showing ZeroTier addresses, versions, roles, latency and path counts.

---

## 2. Post utama — Indonesia

```text
ZeroTier-GUI lama (Python/Tkinter) sudah dormant bertahun-tahun, jadi saya tulis ulang dengan Rust + GPUI 🦀

• network, peer & path
• join/leave + history
• build Linux/macOS/Windows
• GPL-3.0, ide asli Tomás Ralph

github.com/senasgr-eth/zerotier-gui-rs
```

**Gambar:** `img/managenetworks1.png` + `img/networkinfo.png`
**Alt text gambar 1:** Tampilan ZeroTier-GUI dengan daftar network, ID, nama dan status.
**Alt text gambar 2:** Halaman detail network dengan alamat IP dan opsi allowDefault/allowGlobal/allowManaged/allowDNS.

---

## 3. Thread — English (5 post)

**1/5**

```text
I rewrote ZeroTier-GUI in Rust + GPUI 🦀

It's an unofficial desktop front-end for ZeroTier: manage networks, peers and paths without memorising zerotier-cli commands.

Open source, GPL-3.0. The original Python app was made by Tomás Ralph.

🧵
```

**2/5**

```text
The original ZeroTier-GUI had been dormant for years. It worked, but the Python/Tkinter UI felt dated and packaging was painful.

I wanted a native app that starts fast, looks modern, and keeps the same feature set.
```

**3/5**

```text
What it does:

• list, join and leave networks + history
• network details and allow* toggles
• peers with role, latency and paths
• interface and service controls (Linux)
• guided authtoken/permission setup
```

**4/5**

```text
Linux, macOS and Windows builds run in CI, and tagged releases ship a Linux .deb plus portable archives.

Linux gets the full systemd/polkit features. macOS and Windows can still manage networks and peers through zerotier-cli.
```

**5/5**

```text
Code, releases and issues:
github.com/senasgr-eth/zerotier-gui-rs

Original project:
github.com/tralph3/ZeroTier-GUI

Feedback and contributions are welcome. Not affiliated with ZeroTier, Inc.
```

---

## 4. Versi pendek / quote tweet

**English**

```text
The dormant ZeroTier-GUI, rewritten in Rust + GPUI — native desktop app, cross-platform CI, GPL-3.0.

github.com/senasgr-eth/zerotier-gui-rs
```

**Indonesia**

```text
ZeroTier-GUI yang lama, ditulis ulang dengan Rust + GPUI — aplikasi desktop native, CI lintas platform, GPL-3.0.

github.com/senasgr-eth/zerotier-gui-rs
```

---

## 5. Opsi "link di reply"

Kalau mau menghindari link di post utama (beberapa orang percaya reach-nya lebih
baik), posting hook dulu, lalu reply sendiri:

**Post utama**

```text
I rewrote ZeroTier-GUI in Rust + GPUI 🦀

Native desktop app for ZeroTier:
• networks, peers & paths
• join/leave + history
• Linux, macOS & Windows
• GPL-3.0, original by Tomás Ralph
```

**Reply**

```text
Repo: github.com/senasgr-eth/zerotier-gui-rs
Releases: github.com/senasgr-eth/zerotier-gui-rs/releases

Feedback and issues are welcome 🙏
```

---

## 6. Hashtag

Gunakan 2–3 saja, jangan semua:

```text
#ZeroTier #RustLang #OpenSource
```

Alternatif:

```text
#ZeroTier #Rust #GPUI #Linux
```

---

## 7. Saran gambar

- Pakai 2–4 screenshot. Gambar pertama yang paling penting.
- Rekomendasi urutan:
  1. `img/managenetworks1.png` — daftar network
  2. `img/managepeers.png` — peers
  3. `img/networkinfo.png` — detail network
  4. `img/peerpaths.png` — paths
- Selalu isi alt text (contoh di atas).
- Ukuran screenshot 960×680; jangan crop teks table-nya.
- Kalau mau satu gambar collage, gabungkan 4 screenshot jadi grid 2×2.

---

## 8. Checklist sebelum posting

- [ ] Ganti/cek link repo dan release.
- [ ] Lampirkan screenshot, bukan hanya link.
- [ ] Isi alt text untuk setiap gambar.
- [ ] Sebut bahwa ini **unofficial** dan bukan afiliasi ZeroTier, Inc.
- [ ] Kredit proyek asli: Tomás Ralph / `tralph3/ZeroTier-GUI`.
- [ ] Pin post di profil.
- [ ] Balas komentar/issue dengan link repo.
