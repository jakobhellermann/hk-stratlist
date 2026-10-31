/** Absolute URL of a site path (without leading slash), respecting the deploy base path. */
export const url = (path) => `${import.meta.env.BASE_URL.replace(/\/$/, "")}/${path}`;
