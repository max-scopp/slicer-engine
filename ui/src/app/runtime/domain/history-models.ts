export interface RuntimeHistorySession {
  request_uuid: string;
  created_at: string;
  original_filename?: string | null;
  layer_count?: number | null;
  download_url: string;
}
