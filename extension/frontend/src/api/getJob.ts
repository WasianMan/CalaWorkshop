import { axiosInstance } from '@/api/axios.ts';

export type WorkshopJob = {
  id: string;
  state: 'queued' | 'downloading' | 'ready' | 'failed' | string;
  appId: number;
  workshopId: number;
  title?: string | null;
  previewUrl?: string | null;
  installPath?: string | null;
  fileName: string | null;
  files?: string[];
  size: number | null;
  error: string | null;
};

/** Map a raw snake_case job row/view from the backend to the camelCase type. */
export const jobFromWire = (j: any): WorkshopJob => ({
  id: j.id,
  state: j.state,
  appId: j.app_id ?? 0,
  workshopId: Number(j.workshop_id ?? 0),
  title: j.title ?? null,
  previewUrl: j.preview_url ?? null,
  installPath: j.install_path ?? null,
  fileName: j.file_name ?? null,
  files: j.files ?? [],
  size: j.size ?? null,
  error: j.error ?? null,
});

export default async (serverUuid: string, jobId: string): Promise<WorkshopJob> => {
  const { data } = await axiosInstance.get(
    `/api/client/servers/${serverUuid}/calaworkshop/downloads/${jobId}`,
  );
  return jobFromWire(data);
};
