import { axiosInstance } from '@/api/axios.ts';

export type RestoreResult = {
  restored: number;
  file: string;
};

/**
 * Unpack a previously-created Workshop archive back into the server volume and
 * re-track the items it contained. Returns how many registry rows were restored.
 */
export default async (serverUuid: string, file: string): Promise<RestoreResult> => {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/installed/restore`,
    { file },
  );
  return { restored: data.restored ?? 0, file: data.file ?? file };
};
