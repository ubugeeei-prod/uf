package main

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"strings"
	"sync"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
)

type User struct {
	ID                        graphql.ID
	Name, Handle, Avatar, Bio string
	Photo                     *string
}
type account struct {
	User
	Email    string
	Password []byte
}
type Post struct {
	ID                     graphql.ID
	Author                 *User
	Body, Topic, CreatedAt string
	Likes                  int32
	Liked                  bool
}
type storedPost struct {
	Post
	reactions map[graphql.ID]bool
	requestID string
}
type Feed struct {
	Posts   []*Post
	HasNext bool
}
type Thread struct {
	Participant                       *User
	ID                                graphql.ID
	Name, Handle, Avatar, LastMessage string
	Photo                             *string
}
type Message struct {
	ID, ThreadID         graphql.ID
	Author, Body, SentAt string
}
type storedMessage struct {
	Message
	sender    graphql.ID
	requestID string
}
type Conversation struct {
	Thread   *Thread
	Messages []*Message
}
type Settings struct {
	ID                              graphql.ID
	DisplayName, Handle, Bio, Email string
}
type session struct {
	user    graphql.ID
	expires time.Time
}
type Store struct {
	mu       sync.Mutex
	users    map[graphql.ID]*account
	posts    []*storedPost
	sessions map[string]session
	messages map[string][]storedMessage
}

func randomID() string {
	data := make([]byte, 24)
	if _, err := rand.Read(data); err != nil {
		panic(err)
	}
	return hex.EncodeToString(data)
}
func newStore() *Store {
	s := &Store{users: map[graphql.ID]*account{}, sessions: map[string]session{}, messages: map[string][]storedMessage{}}
	people := [][3]string{{"mika", "Mika Tan", "Product engineering"}, {"ren", "Ren Ito", "Design systems"}, {"sora", "Sora Lin", "Developer tools"}, {"niko", "Niko Reyes", "Community"}}
	for _, p := range people {
		photo := "/media/avatars/" + p[0] + ".jpg"
		id := graphql.ID("seed-" + p[0])
		s.users[id] = &account{User: User{id, p[1], p[0], strings.ToUpper(p[0][:1]), p[2], &photo}}
	}
	for i, entry := range [][3]string{
		{"mika", "Removed the company-size question from signup. We never used the answer, and it was the most common place people dropped off.\n\nThe new flow is in staging if anyone has five minutes to try it.", "design"},
		{"ren", "Reading view is live. J / K moves between notes, and the text-size setting now carries across devices.\n\nStill fixing a selection bug in Safari. Please send me a recording if you hit it.", "release"},
		{"sora", "Found the slow query. We were loading every message in a workspace just to show the inbox preview. Down from 840 ms to 46 ms after adding the index and limiting the result.\n\nQuery plan is in the engineering notes.", "runtime"},
		{"niko", "Anyone using a split keyboard? Considering a Corne, but six keys per thumb seems like a lot to learn at once. Curious how long the adjustment took.", "community"},
	} {
		user := s.users[graphql.ID("seed-"+entry[0])].User
		s.posts = append(s.posts, &storedPost{Post: Post{graphql.ID(fmt.Sprint("note-", i)), &user, entry[1], entry[2], "2026-09-11T09:40:00.000Z", 0, false}, reactions: map[graphql.ID]bool{}})
	}
	return s
}

// Every resolver derives identity from the current request's opaque cookie.
func (s *Store) viewer(ctx context.Context) *account {
	rc := ctx.Value(requestKey{}).(*requestContext)
	cookie, err := rc.request.Cookie("commonplace_session")
	if err != nil {
		return nil
	}
	sess, exists := s.sessions[cookie.Value]
	if !exists || time.Now().After(sess.expires) {
		delete(s.sessions, cookie.Value)
		return nil
	}
	return s.users[sess.user]
}
func (s *Store) Viewer(ctx context.Context) *User {
	s.mu.Lock()
	defer s.mu.Unlock()
	if user := s.viewer(ctx); user != nil {
		copy := user.User
		return &copy
	}
	return nil
}
func publicPost(p *storedPost, viewer *account) *Post {
	copy := p.Post
	author := *p.Author
	copy.Author = &author
	copy.Likes = int32(len(p.reactions))
	copy.Liked = viewer != nil && p.reactions[viewer.ID]
	return &copy
}
func (s *Store) Feed(ctx context.Context, args struct {
	Topic, Search string
	Page          int32
}) *Feed {
	s.mu.Lock()
	defer s.mu.Unlock()
	viewer := s.viewer(ctx)
	matched := []*Post{}
	for _, p := range s.posts {
		if args.Topic != "all" && args.Topic != p.Topic {
			continue
		}
		if !strings.Contains(strings.ToLower(p.Body+" "+p.Author.Name+" "+p.Author.Handle), strings.ToLower(args.Search)) {
			continue
		}
		matched = append(matched, publicPost(p, viewer))
	}
	page := max(1, min(args.Page, 1000))
	start := min(int(page-1)*12, len(matched))
	end := min(start+12, len(matched))
	return &Feed{matched[start:end], end < len(matched)}
}
func settings(user *account) *Settings {
	if user == nil {
		return nil
	}
	return &Settings{graphql.ID("settings-" + string(user.ID)), user.Name, user.Handle, user.Bio, user.Email}
}
func (s *Store) Settings(ctx context.Context) *Settings {
	s.mu.Lock()
	defer s.mu.Unlock()
	return settings(s.viewer(ctx))
}

// A private thread belongs to one account and one seed correspondent in this
// fixture. The account's ID is never accepted from GraphQL input.
func (s *Store) thread(viewer *account, id graphql.ID) *Thread {
	if viewer == nil || (id != "thread-mika" && id != "thread-sora") {
		return nil
	}
	user := s.users[graphql.ID(strings.Replace(string(id), "thread-", "seed-", 1))].User
	last := "Welcome to Commonplace. What are you working on?"
	messages := s.messages[string(viewer.ID)+":"+string(id)]
	if len(messages) > 0 {
		last = messages[len(messages)-1].Body
	}
	return &Thread{Participant: &user, ID: id, Name: user.Name, Handle: user.Handle, Avatar: user.Avatar, LastMessage: last, Photo: user.Photo}
}
func (s *Store) Threads(ctx context.Context) []*Thread {
	s.mu.Lock()
	defer s.mu.Unlock()
	result := []*Thread{}
	viewer := s.viewer(ctx)
	for _, id := range []graphql.ID{"thread-mika", "thread-sora"} {
		if thread := s.thread(viewer, id); thread != nil {
			result = append(result, thread)
		}
	}
	return result
}
func (s *Store) Conversation(ctx context.Context, args struct{ ID graphql.ID }) *Conversation {
	s.mu.Lock()
	defer s.mu.Unlock()
	viewer := s.viewer(ctx)
	thread := s.thread(viewer, args.ID)
	if thread == nil {
		return nil
	}
	messages := []*Message{{graphql.ID("welcome-" + string(args.ID)), args.ID, "them", "Welcome to Commonplace. What are you working on?", "2026-09-11T09:00:00.000Z"}}
	for _, entry := range s.messages[string(viewer.ID)+":"+string(args.ID)] {
		copy := entry.Message
		messages = append(messages, &copy)
	}
	return &Conversation{thread, messages}
}
