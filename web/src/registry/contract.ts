import contractData from "@registry-contract";

export interface RegistryContract {
  id: {
    alphabet: string;
    canonicalLength: number;
    prefixLength: number;
  };
  statuses: string[];
  fields: Array<{
    key: string;
    label: string;
    editable: boolean;
    meaningfulFrom?: string;
  }>;
}

export const REGISTRY_CONTRACT = contractData as RegistryContract;
