package main

import (
	"context"
	"crypto/pbkdf2"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"errors"
	"net/http"
	"regexp"
	"strings"
	"time"
	"unicode/utf8"

	graphql "github.com/graph-gophers/graphql-go"
)

var handlePattern = regexp.MustCompile(`^[a-z][a-z0-9_]{2,23}$`)
var unauthenticated = errors.New("sign in to continue")

func passwordHash(password string, salt []byte) []byte {
	key, err := pbkdf2.Key(sha256.New, password, salt, 210000, 32)
	if err != nil {
		panic(err)
	}
	return key
}
func (s *Store) signIn(ctx context.Context, user *account) *User {
	token := randomID()
	expires := time.Now().Add(24 * time.Hour)
	s.sessions[token] = session{user.ID, expires}
	rc := ctx.Value(requestKey{}).(*requestContext)
	http.SetCookie(rc.writer, &http.Cookie{Name: "commonplace_session", Value: token, Path: "/", HttpOnly: true, SameSite: http.SameSiteLaxMode, Expires: expires})
	copy := user.User
	return &copy
}
func (s *Store) Register(ctx context.Context, args struct {
	Input struct{ Name, Handle, Email, Password string }
}) (*User, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	input := args.Input
	name := strings.TrimSpace(input.Name)
	handle := strings.ToLower(strings.TrimSpace(input.Handle))
	if name == "" || utf8.RuneCountInString(name) > 80 || !handlePattern.MatchString(handle) || len(input.Password) < 12 || len(input.Password) > 256 || len(input.Email) > 254 || !strings.Contains(input.Email, "@") {
		return nil, errors.New("use a name, a 3–24 character handle, an email, and a password of at least 12 characters")
	}
	for _, user := range s.users {
		if user.Handle == handle {
			return nil, errors.New("that handle is unavailable")
		}
	}
	salt := make([]byte, 16)
	if _, err := rand.Read(salt); err != nil {
		return nil, err
	}
	user := &account{User: User{graphql.ID(randomID()), name, handle, strings.ToUpper(string([]rune(name)[0])), "", nil}, Email: input.Email, Password: append(salt, passwordHash(input.Password, salt)...)}
	s.users[user.ID] = user
	return s.signIn(ctx, user), nil
}
func (s *Store) Login(ctx context.Context, args struct{ Handle, Password string }) (*User, error) {
	if len(args.Password) > 256 {
		return nil, errors.New("invalid credentials")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, user := range s.users {
		if user.Handle == strings.ToLower(args.Handle) && len(user.Password) == 48 && subtle.ConstantTimeCompare(user.Password[16:], passwordHash(args.Password, user.Password[:16])) == 1 {
			return s.signIn(ctx, user), nil
		}
	}
	return nil, errors.New("invalid credentials")
}
func (s *Store) Logout(ctx context.Context) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	rc := ctx.Value(requestKey{}).(*requestContext)
	if cookie, err := rc.request.Cookie("commonplace_session"); err == nil {
		delete(s.sessions, cookie.Value)
	}
	http.SetCookie(rc.writer, &http.Cookie{Name: "commonplace_session", Path: "/", HttpOnly: true, SameSite: http.SameSiteLaxMode, MaxAge: -1})
	return true
}
func validText(text string, limit int) bool {
	return strings.TrimSpace(text) != "" && utf8.RuneCountInString(text) <= limit
}
func validRequest(id string) bool { return len(id) >= 8 && len(id) <= 128 }
func (s *Store) CreatePost(ctx context.Context, args struct {
	Input struct{ Body, Topic, RequestID string }
}) (*Post, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	user := s.viewer(ctx)
	if user == nil {
		return nil, unauthenticated
	}
	input := args.Input
	if !validText(input.Body, 500) || !validRequest(input.RequestID) || (input.Topic != "design" && input.Topic != "release" && input.Topic != "runtime" && input.Topic != "community") {
		return nil, errors.New("write a note of 1–500 characters and select a channel")
	}
	for _, post := range s.posts {
		if post.Author.ID == user.ID && post.requestID == input.RequestID {
			if post.Body != input.Body || post.Topic != input.Topic {
				return nil, errors.New("this submission ID was already used for another note")
			}
			return publicPost(post, user), nil
		}
	}
	author := user.User
	post := &storedPost{Post: Post{graphql.ID(randomID()), &author, input.Body, input.Topic, time.Now().UTC().Format(time.RFC3339), 0, false}, reactions: map[graphql.ID]bool{}, requestID: input.RequestID}
	s.posts = append([]*storedPost{post}, s.posts...)
	return publicPost(post, user), nil
}
func (s *Store) SetAppreciation(ctx context.Context, args struct {
	ID    graphql.ID
	Liked bool
}) (*Post, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	user := s.viewer(ctx)
	if user == nil {
		return nil, unauthenticated
	}
	for _, post := range s.posts {
		if post.ID == args.ID {
			if args.Liked {
				post.reactions[user.ID] = true
			} else {
				delete(post.reactions, user.ID)
			}
			return publicPost(post, user), nil
		}
	}
	return nil, errors.New("note not found")
}
func (s *Store) SendMessage(ctx context.Context, args struct {
	Input struct {
		ThreadID        graphql.ID
		Body, RequestID string
	}
}) (*Message, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	user := s.viewer(ctx)
	if user == nil {
		return nil, unauthenticated
	}
	input := args.Input
	if s.thread(user, input.ThreadID) == nil || !validText(input.Body, 2000) || !validRequest(input.RequestID) {
		return nil, errors.New("select a conversation and write a message of 1–2000 characters")
	}
	key := string(user.ID) + ":" + string(input.ThreadID)
	for _, previous := range s.messages[key] {
		if previous.requestID == input.RequestID {
			if previous.Body != input.Body {
				return nil, errors.New("this submission ID was already used for another message")
			}
			copy := previous.Message
			return &copy, nil
		}
	}
	message := Message{graphql.ID(randomID()), input.ThreadID, "me", input.Body, time.Now().UTC().Format(time.RFC3339)}
	s.messages[key] = append(s.messages[key], storedMessage{message, user.ID, input.RequestID})
	return &message, nil
}
func (s *Store) UpdateSettings(ctx context.Context, args struct {
	Input struct{ DisplayName, Bio, Email string }
}) (*Settings, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	user := s.viewer(ctx)
	if user == nil {
		return nil, unauthenticated
	}
	input := args.Input
	if !validText(input.DisplayName, 80) || utf8.RuneCountInString(input.Bio) > 240 || len(input.Email) > 254 || !strings.Contains(input.Email, "@") {
		return nil, errors.New("check your display name, biography, and email")
	}
	user.Name = input.DisplayName
	user.Bio = input.Bio
	user.Email = input.Email
	for _, post := range s.posts {
		if post.Author.ID == user.ID {
			copy := user.User
			post.Author = &copy
		}
	}
	return settings(user), nil
}
