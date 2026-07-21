import { useState, useEffect } from "react";
import { Button } from "../components/ui/button";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { FolderOpen, Radio, Music, Info, AlertTriangle } from "lucide-react";

type Track = { hash: string; name: string; path: string; selected?: boolean };

export function StreamBroadcaster() {
  const [enabled, setEnabled] = useState(false);
  const [directory, setDirectory] = useState(() => localStorage.getItem("stream_broadcaster_dir") || "");
  const [tracks, setTracks] = useState<Track[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Restore tracks from backend on mount
  useEffect(() => {
    invoke<Track[]>("get_broadcaster_tracks")
      .then(t => {
        if (t && t.length > 0) {
          // If we got tracks from the backend, it means they are currently selected/active.
          setTracks(t.map(track => ({ ...track, selected: true })));
          setEnabled(true);
        }
      })
      .catch(console.error);
  }, []);

  useEffect(() => {
    if (directory) {
      localStorage.setItem("stream_broadcaster_dir", directory);
      // Only scan automatically if we don't have tracks loaded from backend
      if (tracks.length === 0 && !enabled) {
        scanDirectory(directory);
      }
    }
  }, [directory]);

  const scanDirectory = (dir: string) => {
    setLoading(true);
    setError(null);
    invoke<Track[]>("scan_audio_directory", { path: dir })
      .then(t => setTracks(t.map(track => ({ ...track, selected: true }))))
      .catch(err => {
        console.error(err);
        setError(String(err));
      })
      .finally(() => setLoading(false));
  };

  const toggleTrack = (hash: string) => {
    setTracks(tracks.map(t => t.hash === hash ? { ...t, selected: !t.selected } : t));
  };

  const handleEnableToggle = async (checked: boolean) => {
    setEnabled(checked);
    if (checked) {
      const selectedTracks = tracks.filter(t => t.selected);
      await invoke("set_broadcaster_tracks", { tracks: selectedTracks }).catch(console.error);
    } else {
      await invoke("set_broadcaster_tracks", { tracks: [] }).catch(console.error);
    }
  };

  const handleBrowse = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: directory || undefined,
    });
    if (selected && typeof selected === "string") {
      setDirectory(selected);
      scanDirectory(selected);
    }
  };

  return (
    <main className="min-w-0 flex flex-col h-full overflow-hidden bg-background p-4 gap-4">
      <header className="flex items-center gap-3 border-b border-border pb-3 shrink-0">
        <Radio className="h-6 w-6 text-blue-500" />
        <div className="min-w-0">
          <h1 className="m-0 text-xl font-semibold tracking-normal">Stream Broadcaster</h1>
          <p className="mt-1 text-xs text-muted-foreground">Broadcast an audio directory over the P2P network</p>
        </div>
      </header>
      
      <div className="flex-1 overflow-y-auto space-y-4">
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <Info className="h-4 w-4 text-blue-500" />
              <CardTitle>How it works</CardTitle>
            </div>
          </CardHeader>
          <CardContent className="p-4 text-sm text-muted-foreground space-y-2">
            <p><strong>1.</strong> Connect to the P2P network in the <strong>Rau Connect</strong> tab.</p>
            <p><strong>2.</strong> Select a directory containing <code className="bg-secondary px-1 py-0.5 rounded text-xs">.wav</code> or <code className="bg-secondary px-1 py-0.5 rounded text-xs">.mp3</code> files.</p>
            <p><strong>3.</strong> Enable the broadcaster switch below. Your connected peers can now tune in and stream any track on-demand!</p>
          </CardContent>
        </Card>

        {error && (
          <div className="bg-destructive/10 border border-destructive/20 text-destructive p-3 rounded-md flex items-start gap-2">
            <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
            <div>
              <p className="font-semibold text-sm">Scan Error</p>
              <p className="text-sm opacity-90">{error}</p>
            </div>
          </div>
        )}

        <div className="grid grid-cols-[1fr_auto] gap-2 items-center">
          <div className="flex items-center h-9 w-full rounded-md border border-input bg-card px-3 text-sm text-muted-foreground shadow-sm">
            <FolderOpen className="h-4 w-4 mr-2 shrink-0 opacity-50" />
            <span className="truncate">{directory || "No directory selected"}</span>
          </div>
          <Button onClick={handleBrowse} disabled={enabled} variant={directory ? "secondary" : "default"}>
            Browse
          </Button>
        </div>
        
        <div className="flex items-center justify-between p-3 border border-border rounded-md bg-card">
          <div className="flex flex-col">
            <span className="text-sm font-semibold">Enable Broadcasting</span>
            <span className="text-xs text-muted-foreground">Allow peers to stream selected tracks</span>
          </div>
          <label className="relative inline-flex items-center cursor-pointer">
            <input type="checkbox" className="sr-only peer" checked={enabled} onChange={e => handleEnableToggle(e.target.checked)} disabled={tracks.length === 0} />
            <div className="w-9 h-5 bg-secondary peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-blue-500"></div>
          </label>
        </div>

        {loading && (
          <div className="flex items-center gap-2 p-3 text-sm text-muted-foreground">
            <div className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
            Scanning for audio files...
          </div>
        )}

        {tracks.length > 0 && (
          <Card>
            <CardHeader>
              <div className="flex items-center gap-2">
                <Music className="h-4 w-4 text-emerald-500" />
                <CardTitle>Available Tracks ({tracks.length})</CardTitle>
              </div>
            </CardHeader>
            <CardContent className="p-2 space-y-1">
              {tracks.map(track => (
                <label key={track.hash} className={`flex items-center gap-2 text-sm p-2 rounded cursor-pointer transition-colors ${track.selected ? 'bg-secondary/50 text-foreground' : 'text-muted-foreground hover:bg-secondary/30'}`}>
                  <input 
                    type="checkbox" 
                    checked={track.selected} 
                    onChange={() => toggleTrack(track.hash)} 
                    disabled={enabled}
                    className="rounded border-input text-blue-500 focus:ring-blue-500"
                  />
                  <span className="truncate flex-1">{track.name}</span>
                </label>
              ))}
            </CardContent>
          </Card>
        )}
      </div>
    </main>
  );
}
