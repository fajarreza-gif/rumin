// TypeScript fixture for ai-rumin indexing
export interface UserSession {
    sessionId: string;
    userId: number;
    active: boolean;
}

export type AuthToken = string;

export enum Role {
    Admin,
    Member,
    Guest,
}

export class SessionManager {
    private sessions: Map<string, UserSession> = new Map();

    public async getSession(sessionId: string): Promise<UserSession | undefined> {
        return this.sessions.get(sessionId);
    }
}

export async function authenticateUser(token: AuthToken): Promise<boolean> {
    return token.length > 0;
}

export const generateSessionId = (): string => {
    return "session-" + Math.random().toString(36).substring(2);
};
