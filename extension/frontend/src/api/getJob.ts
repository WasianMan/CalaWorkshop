import { axiosInstance } from '@/api/axios.ts';

export type WorkshopJob = {
  id: string;
  state: 'queued' | 'downloading' | 'ready' | 'failed' | string;
  appId: number;
  workshopId: number;
  title?: string | null;
  previewUrl?: string | null;
  fileName: string | null;
  files?: string[];
  size: number | null;
  error: string | null;
};

export default async (serverUuid: string, jobId: string): Promise<WorkshopJob> => {
  const { data } = await axiosInstance.get(
    `/api/client/servers/${serverUuid}/calaworkshop/downloads/${jobId}`,
  );
  return data;
};
