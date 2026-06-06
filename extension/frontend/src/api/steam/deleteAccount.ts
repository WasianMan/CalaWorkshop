import { axiosInstance } from '@/api/axios.ts';

export default async (label: string): Promise<void> => {
  await axiosInstance.delete(`/api/client/calaworkshop/steam/accounts/${encodeURIComponent(label)}`);
};
