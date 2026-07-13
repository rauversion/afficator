import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { Album } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { cn } from "../../lib/utils";

const coverPathCache = new Map<string, string | null>();
const coverPending = new Map<string, Promise<string | null>>();
const coverQueue: Array<() => void> = [];
let activeCoverRequests = 0;
const maxCoverRequests = 2;

export function TrackCover({
  sourcePath,
  title,
  className
}: {
  sourcePath?: string | null;
  title: string;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement | null>(null);
  const [visible, setVisible] = useState(false);
  const [coverPath, setCoverPath] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    if (typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "160px" }
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    setCoverPath(null);
    setLoaded(false);
    if (!visible || !sourcePath) return;
    let cancelled = false;

    void loadTrackCover(sourcePath).then((path) => {
      if (cancelled) return;
      setCoverPath(path);
      setLoaded(true);
    });

    return () => {
      cancelled = true;
    };
  }, [sourcePath, visible]);

  return (
    <span
      ref={ref}
      className={cn(
        "grid h-10 w-10 shrink-0 place-items-center overflow-hidden rounded-md border border-border bg-secondary text-muted-foreground",
        className
      )}
    >
      {coverPath ? (
        <img src={convertFileSrc(coverPath)} alt={title} className="h-full w-full object-cover" />
      ) : (
        <Album className={cn("h-4 w-4", !loaded && sourcePath && "opacity-50")} />
      )}
    </span>
  );
}

function loadTrackCover(sourcePath: string) {
  if (coverPathCache.has(sourcePath)) {
    return Promise.resolve(coverPathCache.get(sourcePath) ?? null);
  }
  const pending = coverPending.get(sourcePath);
  if (pending) return pending;

  const promise = new Promise<string | null>((resolve) => {
    const run = () => {
      activeCoverRequests += 1;
      invoke<string | null>("playlist_index_track_cover", { sourcePath })
        .then((path) => {
          const normalized = path ?? null;
          coverPathCache.set(sourcePath, normalized);
          resolve(normalized);
        })
        .catch(() => {
          coverPathCache.set(sourcePath, null);
          resolve(null);
        })
        .finally(() => {
          coverPending.delete(sourcePath);
          activeCoverRequests = Math.max(0, activeCoverRequests - 1);
          const next = coverQueue.shift();
          if (next) next();
        });
    };

    if (activeCoverRequests < maxCoverRequests) {
      run();
    } else {
      coverQueue.push(run);
    }
  });
  coverPending.set(sourcePath, promise);
  return promise;
}
