type LogLevel = "debug" | "info" | "warn" | "error";

interface LogEntry {
  level: LogLevel;
  time: number;
  msg: string;
  [key: string]: unknown;
}

class Logger {
  private level: LogLevel;
  private verbose: boolean;

  constructor() {
    this.level =
      (process.env.POLYMARKET_LOG_LEVEL as LogLevel | undefined) || "info";
    this.verbose = false;
  }

  setVerbose(verbose: boolean): void {
    this.verbose = verbose;
    if (verbose) {
      this.level = "debug";
    }
  }

  private shouldLog(level: LogLevel): boolean {
    const levels: LogLevel[] = ["debug", "info", "warn", "error"];
    const currentLevelIndex = levels.indexOf(this.level);
    const messageLevelIndex = levels.indexOf(level);
    return messageLevelIndex >= currentLevelIndex;
  }

  private log(level: LogLevel, msg: string, data?: Record<string, unknown>) {
    if (!this.shouldLog(level)) {
      return;
    }

    const entry: LogEntry = {
      level,
      time: Date.now(),
      msg,
      ...data,
    };

    // Write to stderr for structured logs
    console.error(JSON.stringify(entry));
  }

  debug(msg: string, data?: Record<string, unknown>): void {
    this.log("debug", msg, data);
  }

  info(msg: string, data?: Record<string, unknown>): void {
    this.log("info", msg, data);
  }

  warn(msg: string, data?: Record<string, unknown>): void {
    this.log("warn", msg, data);
  }

  error(msg: string, data?: Record<string, unknown>): void {
    this.log("error", msg, data);
  }
}

export const logger = new Logger();
