import { useEffect, useState } from "react";
import { fetchSchema, ForbiddenError } from "../api/client";
import type { ApiSchema } from "../api/types";

export function useSchema() {
  const [schema, setSchema] = useState<ApiSchema | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [forbiddenUser, setForbiddenUser] = useState<string | null>(null);

  useEffect(() => {
    fetchSchema()
      .then(setSchema)
      .catch((e) => {
        if (e instanceof ForbiddenError) {
          setForbiddenUser(e.username);
        } else {
          setError(e.message);
        }
      });
  }, []);

  return { schema, error, forbiddenUser };
}
