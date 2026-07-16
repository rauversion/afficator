# FFmpeg

Rau Studio for macOS includes the `ffmpeg` and `ffprobe` command-line programs
from FFmpeg 8.1.2. The bundled FFmpeg programs are built with the GPL-licensed
x264 encoder from commit `b35605ace3ddf7c1a5d67a2eb553f034aef41d55`, the
LGPL-licensed LAME 3.101 MP3 encoder, network protocols, and macOS
SecureTransport support. They do not include non-free components.

This FFmpeg configuration and x264 are distributed under the GNU General Public
License version 2 or later. LAME is distributed under the GNU Lesser General
Public License version 2. Their license texts are included beside this notice in
the application resources.

Corresponding source code:

https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz

https://code.videolan.org/videolan/x264/-/archive/b35605ace3ddf7c1a5d67a2eb553f034aef41d55/x264-b35605ace3ddf7c1a5d67a2eb553f034aef41d55.tar.bz2

https://downloads.sourceforge.net/project/lame/lame/3.101/lame-3.101.tar.gz

The same source archives are attached to each Rau Studio release that
distributes these binaries. The exact configure options are recorded in
`scripts/prepare-ffmpeg-sidecars.sh`.
