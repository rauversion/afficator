# Radio Broadcast

Rau Studio can publish playlists stored on the local computer as a continuous
MP3 stream to an Icecast server. The Mac remains the audio source; Icecast owns
the public URL and distributes the stream to listeners.

## Recommended Home-to-World Topology

```text
Mac at home (Rau Studio + FFmpeg)
          | outbound source connection
          v
Public Icecast server or hosted Icecast account
          | https://radio.example.com/live.mp3
          v
Listeners anywhere
```

Using a remote Icecast server is the simplest setup. Rau Studio only opens an
outbound connection, so the home router does not need port forwarding and a
carrier-grade NAT connection is not a problem.

Icecast can alternatively run on the home network, but then its listener port
must be reachable from the internet. That normally requires router port
forwarding, firewall rules, dynamic DNS or a static address, and an ISP that
does not place the connection behind CGNAT. Put TLS in front of a public home
server; do not expose an unprotected Icecast admin interface.

## Prerequisites

- A reachable Icecast 2 server or hosted Icecast account.
- Source host, port, mountpoint, username, and source password.
- At least one Rekordbox XML library indexed under **Playlist Library**.
- Local source files that still exist at their indexed paths.
- Upload bandwidth above the selected bitrate. Leave headroom for reconnects
  and other traffic; a 128 kbps station uses roughly 58 MB per hour of upload.
- macOS 13 or newer. Capturing the Mac's complete output or one application's
  output uses ScreenCaptureKit and the system's Screen & System Audio Recording
  permission.

The signed macOS build includes FFmpeg with `libmp3lame` and the
`icecast/http/https/tcp/tls` protocols. A manually selected FFmpeg build must
provide the same capabilities.

## Configure and Start

1. Open **Broadcast** in the Studio sidebar.
2. Enter the Icecast destination:
   - **Host**: hostname only, without `http://`, path, or credentials.
   - **Port**: commonly `8000` without TLS or `443`/provider-specific with TLS.
   - **Mountpoint MP3**: for example `/live.mp3`.
   - **Source user**: commonly `source`, unless the provider says otherwise.
   - **Source password**: the source credential, not the admin password.
   - **Use TLS**: enable only when the endpoint accepts secure source traffic.
3. Choose an MP3 bitrate from 96 to 320 kbps and save the profile.
4. Optionally enable **Preparar micrófono al iniciar**, choose the input device,
   and set its gain. The microphone always starts muted for privacy.
5. Optionally enable **Preparar línea directa al iniciar**, choose an audio
   interface, select a mono channel or stereo pair, and set its gain. Line input
   is prepared in standby and never starts live automatically.
6. Optionally enable **Preparar salida del Mac al iniciar** and leave **Toda la
   salida del Mac** selected to broadcast the computer's normal output. You can
   instead restrict capture to one open application. Set its gain; this source
   is prepared in standby and never starts live automatically.
   These three sources are organized as tabs in the Icecast destination form;
   the green dot identifies sources configured for the next broadcast start.
7. Confirm that the FFmpeg preflight reports ready.
8. Select an indexed library and playlist, then choose **Agregar**. Adding more
   playlists appends them to the existing queue.
   Individual indexed-track rows throughout Rau Studio also expose **Agregar al
   broadcast**, which appends only that track to the same durable queue.
9. Choose **Salir al aire**. The status changes through connecting to live.
10. Use **Micrófono al aire** only while speaking, then choose
   **Silenciar micrófono**.
11. Use **Línea directa al aire** to temporarily replace the playlist with the
    selected hardware input. Choose **Volver a Playlist** to resume the held
    track and queue.
12. Use **Salida del Mac al aire** to replace the playlist with the Mac's stereo
    output. Choose **Volver a Playlist** to resume.
13. Test the displayed listener URL in another device or network.

The queue is durable in SQLite. Played, skipped, and failed rows remain visible
until cleared. The active row cannot be removed, but it can be skipped.

## Runtime Behavior

- Each local file is decoded to stereo 44.1 kHz PCM, regardless of its original
  format, then encoded to constant-bitrate MP3 by the persistent publisher.
- Icecast receives one continuous source connection across track transitions.
- When the queue runs out, Rau Studio transmits silence rather than closing the
  mount. New playlists can be appended while the station is live.
- Artist and title metadata are sent as UTF-8 when a track starts. Icecast
  exposes the current value on its status page and through `/status-json.xsl`.
- The selected microphone is captured natively through CPAL/CoreAudio,
  normalized and resampled to the same stereo 44.1 kHz PCM format, and mixed
  with the track or idle silence. Gain is limited to 0–200%, and sample sums are
  clamped to avoid integer overflow. Voice-activated ducking lowers music to
  35% while speech is detected, then restores it gradually so speech is not
  buried under a mastered track and level changes do not click or pump. The
  bounded buffer keeps a 250 ms reserve to absorb CoreAudio/FFmpeg callback
  jitter, avoids unbounded latency or memory, and the control panel displays
  its live input level.
- Direct line is a separate primary-source mode. It selects one mono channel
  (duplicated to both output channels) or an adjacent stereo pair from any
  CoreAudio input device, normalizes it to stereo 44.1 kHz PCM, and sends it to
  the persistent Icecast publisher without voice detection or ducking. While
  direct line is live, the current playlist decoder is held by backpressure and
  the queue does not advance. Returning to Playlist resumes that decoder. The
  microphone is muted and unavailable while direct line is the active source.
- Mac output is a third, mutually exclusive primary-source mode on macOS.
  ScreenCaptureKit captures the complete system output by default, or filters
  it to one selected running application, provides stereo 48 kHz samples, and Rau Studio
  resamples them to the publisher's stereo 44.1 kHz PCM stream. It does not
  capture the microphone, mix the playlist, or apply ducking. As with line
  input, the playlist decoder and queue wait until the operator returns to
  Playlist. Rau Studio excludes its own audio to avoid feedback. When capture
  is restricted to an application, that application must be open when the
  broadcast starts.
- On a broken source connection the publisher retries. A track interrupted by
  that failure returns to the queue.
- Closing Rau Studio ends the local source process. Icecast then removes the
  live mount unless it has its own fallback mount configured.

## Security and Operational Notes

- The source password is encrypted at rest and the frontend receives only a
  `password configured` flag. FFmpeg still receives the credential locally
  while the source process runs, so other administrator-level processes on the
  same computer may be able to inspect it.
- Prefer TLS whenever the Icecast service supports it. Without TLS, source
  credentials and audio cross the network without transport encryption.
- Do not use the Icecast admin password as the source password.
- Only broadcast audio you are authorized to distribute. Music licensing and
  royalty obligations depend on the countries and audience involved.
- macOS asks for microphone access the first time capture starts. If it was
  denied, enable Rau Studio under **System Settings → Privacy & Security →
  Microphone**, then restart the app.
- Listing applications or capturing Mac output asks for **Screen & System Audio
  Recording** access. If denied, enable Rau Studio under **System Settings →
  Privacy & Security → Screen & System Audio Recording**, restart the app,
  and press **Refrescar**. Open the source program first only when restricting
  capture to one application.

## Troubleshooting

**FFmpeg is not ready**

Run `npm run sidecars:prepare` for a source build, or select an FFmpeg binary in
Settings that exposes `libmp3lame` and the `icecast` protocol.

**The station reconnects repeatedly**

Check the host, port, mountpoint, source username/password, and TLS setting.
Icecast logs usually distinguish authentication failures from duplicate mounts
or unsupported TLS.

**Listeners cannot open the URL**

The source connection and listener endpoint are separate checks. Confirm that
the Icecast listener port and mount are publicly reachable. For a home-hosted
server, also verify port forwarding, firewall, public IP, and CGNAT status.

**A playlist adds fewer tracks than expected**

Tracks without a current local source path are omitted. Reindex the library
after reconnecting external drives or moving files.

**No audio after the last track**

Silence is expected while the queue is empty. Append another playlist or stop
the broadcast explicitly.

**The Mac output meter stays at zero**

Confirm the macOS Screen & System Audio Recording permission, restart Rau Studio
after changing it, and play audio in any program. If capture is restricted to
one application, open that program before refreshing the list and play audio in
it; that mode intentionally ignores audio from other programs.
