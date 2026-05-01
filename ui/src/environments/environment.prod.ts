type Environment = {
  production: boolean;
  apiUrl: string;
  wsUrl: string;
  sliceBackend: 'server' | 'wasm';
};

export const environment: Environment = {
  production: true,
  apiUrl: '/api',
  wsUrl: 'ws://' + window.location.host,
  sliceBackend: 'server',
};
