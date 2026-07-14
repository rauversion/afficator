import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { Album } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { cn } from "../../lib/utils";

const coverPathCache = new Map<string, string | null>();
const coverPending = new Map<string, PendingCoverRequest>();
const coverQueue: PendingCoverRequest[] = [];
const coverVisibilityCallbacks = new WeakMap<Element, () => void>();
let activeCoverRequests = 0;
let coverVisibilityObserver: IntersectionObserver | null = null;
const maxCoverRequests = 2;

type PendingCoverRequest = {
  cancelled: boolean;
  consumers: number;
  promise: Promise<string | null>;
  resolve: (path: string | null) => void;
  sourcePath: string;
  started: boolean;
};

type CoverRequestSubscription = {
  cancel: () => void;
  promise: Promise<string | null>;
};

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
    return observeCoverVisibility(node, () => setVisible(true));
  }, []);

  useEffect(() => {
    setCoverPath(null);
    setLoaded(false);
    if (!visible || !sourcePath) return;
    let cancelled = false;

    const request = loadTrackCover(sourcePath);
    void request.promise.then((path) => {
      if (cancelled) return;
      setCoverPath(path);
      setLoaded(true);
    });

    return () => {
      cancelled = true;
      request.cancel();
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
        <img
          src={convertFileSrc(coverPath)}
          alt={title}
          loading="lazy"
          decoding="async"
          className="h-full w-full object-cover"
        />
      ) : (
        <Album className={cn("h-4 w-4", !loaded && sourcePath && "opacity-50")} />
      )}
    </span>
  );
}

function observeCoverVisibility(node: Element, onVisible: () => void) {
  if (typeof IntersectionObserver === "undefined") {
    onVisible();
    return undefined;
  }

  if (!coverVisibilityObserver) {
    coverVisibilityObserver = new IntersectionObserver(
      (entries, observer) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          coverVisibilityCallbacks.get(entry.target)?.();
          coverVisibilityCallbacks.delete(entry.target);
          observer.unobserve(entry.target);
        });
      },
      { rootMargin: "160px" }
    );
  }

  coverVisibilityCallbacks.set(node, onVisible);
  coverVisibilityObserver.observe(node);
  return () => {
    coverVisibilityCallbacks.delete(node);
    coverVisibilityObserver?.unobserve(node);
  };
}

function loadTrackCover(sourcePath: string): CoverRequestSubscription {
  if (coverPathCache.has(sourcePath)) {
    return {
      cancel: () => undefined,
      promise: Promise.resolve(coverPathCache.get(sourcePath) ?? null)
    };
  }

  let request = coverPending.get(sourcePath);
  if (!request) {
    let resolveRequest = (_path: string | null) => undefined;
    const promise = new Promise<string | null>((resolve) => {
      resolveRequest = resolve;
    });
    request = {
      cancelled: false,
      consumers: 0,
      promise,
      resolve: resolveRequest,
      sourcePath,
      started: false
    };
    coverPending.set(sourcePath, request);
    enqueueCoverRequest(request);
  }

  request.consumers += 1;
  let cancelled = false;
  return {
    promise: request.promise,
    cancel: () => {
      if (cancelled) return;
      cancelled = true;
      request.consumers = Math.max(0, request.consumers - 1);
      if (request.consumers === 0 && !request.started) {
        request.cancelled = true;
        if (coverPending.get(sourcePath) === request) {
          coverPending.delete(sourcePath);
        }
        request.resolve(null);
      }
    }
  };
}

function enqueueCoverRequest(request: PendingCoverRequest) {
  if (activeCoverRequests < maxCoverRequests) {
    runCoverRequest(request);
  } else {
    coverQueue.push(request);
  }
}

function runCoverRequest(request: PendingCoverRequest) {
  if (request.cancelled) {
    runNextCoverRequest();
    return;
  }

  request.started = true;
  activeCoverRequests += 1;
  invoke<string | null>("playlist_index_track_cover", { sourcePath: request.sourcePath })
    .then((path) => {
      const normalized = path ?? null;
      coverPathCache.set(request.sourcePath, normalized);
      request.resolve(normalized);
    })
    .catch(() => {
      coverPathCache.set(request.sourcePath, null);
      request.resolve(null);
    })
    .finally(() => {
      if (coverPending.get(request.sourcePath) === request) {
        coverPending.delete(request.sourcePath);
      }
      activeCoverRequests = Math.max(0, activeCoverRequests - 1);
      runNextCoverRequest();
    });
}

function runNextCoverRequest() {
  while (activeCoverRequests < maxCoverRequests) {
    const next = coverQueue.shift();
    if (!next) return;
    if (!next.cancelled) {
      runCoverRequest(next);
    }
  }
}
