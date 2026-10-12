package service

import (
	"context"
	"errors"
)

type UserStore interface {
	GetUser(ctx context.Context, id int64) (*User, error)
	SaveUser(ctx context.Context, user *User) error
}

type User struct {
	ID    int64
	Name  string
	Email string
}

type UserService struct {
	store UserStore
}

func NewUserService(store UserStore) *UserService {
	return &UserService{store: store}
}

func (s *UserService) FetchUserProfile(ctx context.Context, id int64) (*User, error) {
	if id <= 0 {
		return nil, errors.New("invalid id")
	}
	return s.store.GetUser(ctx, id)
}
