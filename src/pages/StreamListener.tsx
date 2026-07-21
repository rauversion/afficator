import { useState, useRef, useEffect } from "react";
import { Button } from "../components/ui/button";
import { invoke } from "@tauri-apps/api/core";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Headphones, KeyRound, Play, Music, Info, AlertTriangle } from "lucide-react";

type Track = { hash: string; name: string; path: string };

export function StreamListener() {
  const [ticket, setTicket] = useState(() => localStorage.getItem("stream_listener_ticket") || "");
  const [tracks, setTracks] = useState<Track[]>([]);
  const [activeTrack, setActiveTrack] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);

  useEffect(() => {
    localStorage.setItem("stream_listener_ticket", ticket);
  }, [ticket]);

  const handleLoad = async () => {
    if (!ticket) return;
    setLoading(true);
    setError(null);
    try {
      const fetchedTracks = await invoke<Track[]>("connect_to_broadcaster", { ticket });
      setTracks(fetchedTracks);
      // Automatically play the first track if available
      if (fetchedTracks.length > 0) {
        setActiveTrack(fetchedTracks[0].hash);
      }
    } catch (e) {
      console.error(e);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleNextTrack = () => {
    if (!activeTrack || tracks.length === 0) return;
    const currentIndex = tracks.findIndex(t => t.hash === activeTrack);
    if (currentIndex >= 0 && currentIndex < tracks.length - 1) {
      setActiveTrack(tracks[currentIndex + 1].hash);
    } else {
      setActiveTrack(null);
    }
  };

  return (
    <main className="min-w-0 flex flex-col h-full overflow-hidden bg-background p-4 gap-4 pb-28">
      <header className="flex items-center gap-3 border-b border-border pb-3 shrink-0">
        <Headphones className="h-6 w-6 text-emerald-500" />
        <div className="min-w-0">
          <h1 className="m-0 text-xl font-semibold tracking-normal">Stream Listener</h1>
          <p className="mt-1 text-xs text-muted-foreground">Tune in to a peer's live P2P audio stream</p>
        </div>
      </header>

      <div className="flex-1 overflow-y-auto space-y-4">
        {tracks.length === 0 && (
          <Card>
            <CardHeader>
              <div className="flex items-center gap-2">
                <Info className="h-4 w-4 text-emerald-500" />
                <CardTitle>How it works</CardTitle>
              </div>
            </CardHeader>
            <CardContent className="p-4 text-sm text-muted-foreground space-y-2">
              <p><strong>1.</strong> Ask the broadcaster to start their stream and copy their <strong>Rau Connect Ticket</strong>.</p>
              <p><strong>2.</strong> Paste the ticket below and click Load to fetch their live playlist.</p>
              <p><strong>3.</strong> The music will stream directly from their device to yours in real-time!</p>
            </CardContent>
          </Card>
        )}

        {error && (
          <div className="bg-destructive/10 border border-destructive/20 text-destructive p-3 rounded-md flex items-start gap-2">
            <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
            <div>
              <p className="font-semibold text-sm">Connection Error</p>
              <p className="text-sm opacity-90">{error}</p>
            </div>
          </div>
        )}

        <div className="grid grid-cols-[1fr_auto] gap-2 items-center">
          <div className="flex items-center h-9 w-full rounded-md border border-input bg-card px-3 text-sm text-muted-foreground shadow-sm focus-within:ring-1 focus-within:ring-ring">
            <KeyRound className="h-4 w-4 mr-2 shrink-0 opacity-50" />
            <input 
              type="text" 
              placeholder="Paste Broadcaster Ticket here..." 
              value={ticket} 
              onChange={e => setTicket(e.target.value)} 
              className="flex-1 bg-transparent border-none focus:outline-none text-foreground placeholder:text-muted-foreground"
              disabled={loading}
            />
          </div>
          <Button onClick={handleLoad} disabled={loading || !ticket} variant={ticket ? "default" : "secondary"}>
            {loading ? (
              <span className="flex items-center gap-2">
                <div className="h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent" />
                Connecting
              </span>
            ) : "Load Stream"}
          </Button>
        </div>
        
        {tracks.length > 0 && (
          <Card>
            <CardHeader>
              <div className="flex items-center gap-2">
                <Music className="h-4 w-4 text-emerald-500" />
                <CardTitle>Broadcaster Playlist ({tracks.length})</CardTitle>
              </div>
            </CardHeader>
            <CardContent className="p-2 space-y-1 max-h-[50vh]">
              {tracks.map((track) => (
                <div 
                  key={track.hash} 
                  className={`flex items-center justify-between p-2 rounded transition-colors ${activeTrack === track.hash ? 'bg-secondary/70 border-primary shadow-sm' : 'hover:bg-secondary/30'}`}
                >
                  <span className={`text-sm font-medium truncate flex-1 ${activeTrack === track.hash ? 'text-foreground' : 'text-muted-foreground'}`}>
                    {track.name}
                  </span>
                  <div className="flex gap-2 items-center shrink-0 ml-4">
                    <Button 
                      size="sm" 
                      variant={activeTrack === track.hash ? "default" : "secondary"} 
                      onClick={() => setActiveTrack(track.hash)}
                      className="h-7 text-xs px-3"
                    >
                      {activeTrack === track.hash ? "Playing" : <><Play className="h-3 w-3 mr-1" /> Play</>}
                    </Button>
                  </div>
                </div>
              ))}
            </CardContent>
          </Card>
        )}
      </div>

      {activeTrack && (
        <div className="fixed bottom-0 left-0 right-0 bg-card/95 backdrop-blur supports-[backdrop-filter]:bg-card/80 p-4 border-t border-border flex flex-col items-center z-50 shadow-[0_-4px_16px_rgba(0,0,0,0.1)]">
          <p className="text-sm font-semibold mb-2 truncate max-w-xl text-center">
            {tracks.find(t => t.hash === activeTrack)?.name}
          </p>
          <audio 
            ref={audioRef}
            controls 
            autoPlay 
            src={`http://127.0.0.1:4000/stream?hash=${activeTrack}`} 
            onEnded={handleNextTrack}
            className="w-full max-w-2xl h-10 outline-none"
          />
        </div>
      )}
    </main>
  );
}
