package com.airmem.service;

import java.util.Optional;

public interface UserRepository {
    Optional<String> findEmailById(long id);
}

public enum AccountStatus {
    ACTIVE,
    SUSPENDED,
    CLOSED
}

public record UserProfile(long id, String email, AccountStatus status) {}

public class UserService {
    private final UserRepository repository;

    public UserService(UserRepository repository) {
        this.repository = repository;
    }

    public UserProfile fetchUserProfile(long id) {
        String email = this.repository.findEmailById(id).orElse("unknown@example.com");
        return new UserProfile(id, email, AccountStatus.ACTIVE);
    }
}
