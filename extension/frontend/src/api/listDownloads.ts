import { axiosInstance } from '@/api/axios.ts';
import { jobFromWire, type WorkshopJob } from './getJob.ts';

export type DownloadsList = {
  /** In-flight jobs (queued / downloading / ready), always returned in full. */
  active: WorkshopJob[];
  /** One page of terminal jobs (installed / failed), newest first. */
  history: WorkshopJob[];
  historyTotal: number;
  page: number;
  perPage: number;
};

export default async (serverUuid: string, page = 1): Promise<DownloadsList> => {
  const { data } = await axiosInstance.get(
    `/api/client/servers/${serverUuid}/calaworkshop/downloads`,
    { params: { page } },
  );
  return {
    // `jobs` is the deprecated back-compat alias for `active`.
    active: (data.active ?? data.jobs ?? []).map(jobFromWire),
    history: (data.history ?? []).map(jobFromWire),
    historyTotal: data.history_total ?? 0,
    page: data.page ?? page,
    perPage: data.per_page ?? 25,
  };
};
