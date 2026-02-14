import { useEffect, useState } from 'react';
import { fetchSchema } from '../api/client';
import type { ApiSchema } from '../api/types';

export function useSchema() {
  const [schema, setSchema] = useState<ApiSchema | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchSchema()
      .then(setSchema)
      .catch((e) => setError(e.message));
  }, []);

  return { schema, error };
}
