export type TrackListItem = {
  library_id?: string;
  track_id: string;
  name?: string | null;
  artist?: string | null;
  album?: string | null;
  genre?: string | null;
  comments?: string | null;
  bpm?: string | null;
  key?: string | null;
  rating?: string | null;
  user_rating?: number | null;
  year?: string | null;
  label?: string | null;
  date_added?: string | null;
  kind?: string | null;
  location?: string | null;
  source_path?: string | null;
  source_exists: boolean;
  total_time?: number | null;
  attributes?: Record<string, string>;
  embedding_ready?: boolean;
};

export type TrackListColumn =
  | "artist"
  | "album"
  | "genre"
  | "bpm"
  | "key"
  | "rating"
  | "year"
  | "label"
  | "comments"
  | "kind";

export type TrackPlaybackContext = {
  id: string;
  label?: string | null;
  tracks: TrackListItem[];
};
