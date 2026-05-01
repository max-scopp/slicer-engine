const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
const httpProtocol = window.location.protocol === 'https:' ? 'https:' : 'http:';

type Environment = {
  production: boolean;
  apiUrl: string;
  wsUrl: string;
  sliceBackend: 'server' | 'wasm';
};

export const environment: Environment = {
  production: true,
  apiUrl: `${httpProtocol}//${window.location.host}/api`,
  wsUrl: `${wsProtocol}//${window.location.host}/ws`,
  sliceBackend: 'wasm',
};