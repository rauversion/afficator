# Broadcast

Rau Studio can publish playlists and local audio inputs in two modes:

- **Icecast** sends one continuous MP3 source stream to a radio server. Icecast
  owns the public listener URL and distributes the audio.
- **RTMP/RTMPS** turns the audio into a vertical H.264/AAC video signal for a
  live-video service. The Instagram preset is designed for Live Producer.

Both modes use the same durable queue, microphone, direct line, and Mac-output
controls. The Mac remains the source and opens only an outbound connection.

## Icecast Home-to-World Topology

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

- For Icecast, a reachable Icecast 2 server or hosted account plus its source
  host, port, mountpoint, username, and password.
- For Instagram, an account with access to Live Producer on Instagram.com. The
  server URL and stream key are created by Instagram for each Live.
- At least one Rekordbox XML library indexed under **Playlist Library**.
- Local source files that still exist at their indexed paths.
- Upload bandwidth above the selected bitrate. Leave headroom for reconnects
  and other traffic. A 128 kbps station uses roughly 58 MB per hour; a 3.5 Mbps
  video signal uses roughly 1.6 GB per hour.
- macOS 13 or newer for Mac audio capture. Capturing the Mac's complete output or one application's
  output uses ScreenCaptureKit and the system's Screen & System Audio Recording
  permission.

The signed macOS build includes FFmpeg with `libmp3lame`, `libx264`, AAC, the
FLV muxer, the `testsrc2` filter, and the
Icecast/RTMP/RTMPS network protocols.
A manually selected FFmpeg build must provide the capabilities required by the
selected destination. RTMP station and track typography additionally requires
the `drawtext` filter. The Homebrew FFmpeg build includes it; builds without it
fall back to the same animated graphic without text.

## Configure and Start Icecast

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
   These three sources are organized as tabs in the destination form;
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

The queue is durable in SQLite. Each non-playing row has **Play now**, which
cuts the current decoder and starts the selected track without reconnecting the
destination. Queued rows can be dragged, moved with the arrow controls, or
sorted by title, artist, and duration. Played, skipped, and failed rows remain
visible until cleared. The active row cannot be removed or reordered.

## Configure and Start Instagram Live

1. On desktop, open Instagram.com and create a new **Live video**. Live Producer
   shows the server URL and stream key. Keep that page open.
2. In Rau Studio, open **Broadcast**, choose **RTMP / RTMPS · Video en vivo**,
   and select **Instagram Live**.
3. Paste the Instagram server URL. Save the Broadcast profile. Rau Studio
   persists this URL and the video/audio bitrates, but never persists the
   stream key.
4. Paste the stream key into **Clave de transmisión · solo esta sesión**. It is
   kept only in the current frontend session and cleared when the broadcast is
   stopped.
5. Optional: open **Video Studio** and enable **Camera**, **Screen / window**, or both. The system picker lets you choose
   an entire display or one application window. Select either layer to edit its Card, Full width, or Background layout;
   fit/crop framing, orientation, effect, mirror, opacity, position, and size are independent. The combined visual stays
   out of Program when the broadcast starts.
6. Add tracks to the queue, configure any local inputs, confirm the FFmpeg
   preflight is ready, and choose **Salir al aire**.
7. Rau Studio sends a 720 × 1280, 30 fps H.264 video with AAC audio and an
   independently paced monochrome broadcast graphic. It shows the configured
   station name, encoding information, and the current artist/title. Wait for
   the image to appear in Live Producer.
8. Open **Video Studio** while the signal is running. **PREVIEW** and **PROGRAM**
   show the live camera and selected display/window layers; **PROGRAM** represents the combined composition being sent. Use the
   fader for an immediate manual mix, or **AUTO** for the saved timed dissolve.
   Returning the fader to zero makes the visual layer transparent while the
   branded RTMP video and selected capture continue uninterrupted.
9. Review the preview, title, and audience in Instagram, then click **Go live**
   there. Starting the signal in Rau Studio does not publish the Live by itself.
10. To finish, end the Live in Instagram first and then stop Broadcast in Rau
   Studio. This avoids leaving Instagram waiting on an abruptly closed signal.

Instagram controls account eligibility, feature availability, preview timing,
and the validity window of its credentials. If Live Producer provides a new
URL or key, use the new values. The implementation follows Instagram's
[Live Producer workflow](https://about.instagram.com/blog/tips-and-tricks/instagram-live-producer).

For another service, choose **RTMP personalizado**. It accepts `rtmp://` or
`rtmps://` endpoints and keeps the same vertical H.264/AAC scene; confirm the
service's bitrate, resolution, and keyframe requirements before going live.

## Runtime Behavior

- Each local file is decoded to stereo 44.1 kHz PCM, regardless of its original
  format, then written to one persistent publisher process.
- Icecast encodes that PCM as constant-bitrate MP3. RTMP encodes the audio as
  AAC and pairs it with an independently paced H.264 graphic so video generation
  cannot block the audio pipe.
- In RTMP mode, artist and title are written atomically to a temporary UTF-8
  text file. FFmpeg reloads it while the publisher remains open, so track
  transitions, direct line input, Mac audio, and the idle state update on screen
  without interrupting the Live.
- When the visual compositor is enabled, the webview keeps the camera and the OS-selected display/window streams alive
  independently. It draws both sources into a transparent 360 × 640 canvas and submits paced WebP snapshots through local
  Tauri IPC. A persistent FFmpeg `image2pipe` decoder converts the latest composition to BGRA for the publisher's named pipe.
  At zero mix the combined layer is
  transparent, while each active source remains warm for a clean
  take. Moving the fader changes the layer alpha frame by frame without
  replacing the publisher. Rau detects missing or repeated frames and restarts
  only visual capture when it freezes.
- Camera and screen/window activation, device, composition, position, size, fit/crop framing, orientation, mirror, effect, maximum opacity, and AUTO
  duration are persisted with the Broadcast profile and can be changed while live. Full width spans the 9:16
  canvas; Background places the source beneath the compact Rau identity and track information. The display/window layer is
  drawn first and the camera layer above it. Layer changes redraw the transparent canvas; opacity and Preview/Program mix
  update directly without reconnecting RTMP. Browser media capture keeps this path common across macOS, Windows, and Linux.
- The destination receives one continuous connection across track transitions.
  When the queue runs out, Rau Studio transmits silence rather than closing the
  connection. New playlists can be appended while it is live.
- RTMP processes server control messages after every muxed packet and reports
  `connected` only after at least two seconds of media have advanced. Opening
  the destination is reported separately while the preview is prepared.
- In Icecast mode, artist and title metadata are sent as UTF-8 when a track
  starts. Icecast exposes the current value on its status page and through
  `/status-json.xsl`. RTMP mode does not send Icecast metadata updates.
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
  the persistent publisher without voice detection or ducking. While
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
- Closing Rau Studio ends the local publisher process. Icecast then removes the
  live mount unless it has its own fallback mount configured; an RTMP platform
  sees the incoming signal disconnect.

## Security and Operational Notes

- The source password is encrypted at rest and the frontend receives only a
  `password configured` flag. FFmpeg still receives the credential locally
  while the source process runs, so other administrator-level processes on the
  same computer may be able to inspect it.
- RTMP stream keys are deliberately not written to SQLite or the encrypted
  settings vault. The key is passed to the local FFmpeg process for the active
  session, so administrator-level processes may still be able to inspect its
  command line while the signal is running. Treat a stream key as a password
  and revoke or regenerate it if it is exposed.
- Prefer TLS whenever the Icecast service supports it. Without TLS, source
  credentials and audio cross the network without transport encryption.
- Do not use the Icecast admin password as the source password.
- Only broadcast audio you are authorized to distribute. Music licensing and
  royalty obligations depend on the countries and audience involved.
- Instagram can limit, mute, block, or end Lives based on music rights and how
  music is used. Review Meta's current
  [Music Guidelines](https://www.facebook.com/legal/music_guidelines) before
  publishing DJ sets or other music-heavy streams.
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
Settings. Icecast requires `libmp3lame` and the Icecast protocol. RTMP requires
`libx264`, AAC, FLV, `testsrc2`, and the RTMP or RTMPS
protocol selected by the destination. Select a build with `drawtext`—such as
Homebrew FFmpeg—to include station and track information in the video. Without
it, Rau keeps RTMP available but sends the graphic without typography.

**Instagram does not show a preview**

Confirm that the server URL starts with `rtmps://`, paste the current Live's
stream key again, and check the Rau terminal for reconnect messages. A key from
an older Live may no longer be valid. Rau Studio only sends the signal; the
operator must still click **Go live** in Live Producer after the preview loads.

**The camera preview is unavailable**

Allow Camera access for Rau Studio in macOS Privacy & Security, close the app
completely, and reopen it. Confirm that the camera is not already locked by
another application. The local PREVIEW releases its browser capture before
FFmpeg takes the camera into PROGRAM.

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
