import helpers = require("./helpers");
declare function createServer(options?: createServer.Options): createServer.Server;
declare namespace createServer {
    interface Options {
        port: number;
    }
    interface Server {
        listen(): void;
        helpers: typeof helpers;
    }
    type Handler<T = string> = (request: T) => void;
}
export = createServer;
