package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func execute(t *testing.T, handler http.Handler, query string, cookie *http.Cookie) (map[string]interface{}, *http.Cookie) {
	t.Helper()
	body, _ := json.Marshal(map[string]string{"query": query})
	request := httptest.NewRequest(http.MethodPost, "/", bytes.NewReader(body))
	request.Header.Set("Content-Type", "application/json")
	if cookie != nil {
		request.AddCookie(cookie)
	}
	response := httptest.NewRecorder()
	handler.ServeHTTP(response, request)
	if response.Code != 200 {
		t.Fatalf("status %d: %s", response.Code, response.Body.String())
	}
	var payload map[string]interface{}
	if err := json.Unmarshal(response.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	var session *http.Cookie
	if cookies := response.Result().Cookies(); len(cookies) > 0 {
		session = cookies[0]
	}
	return payload, session
}
func registerAccount(t *testing.T, handler http.Handler, handle string) *http.Cookie {
	t.Helper()
	result, cookie := execute(t, handler, `mutation { register(input:{name:"Test Member", handle:"`+handle+`",email:"member@example.com",password:"test-only-long-password"}) { id } }`, nil)
	if result["errors"] != nil || cookie == nil || !cookie.HttpOnly {
		t.Fatalf("register failed: %v", result)
	}
	return cookie
}
func field(t *testing.T, result map[string]interface{}, name string) map[string]interface{} {
	t.Helper()
	if result["errors"] != nil {
		t.Fatalf("operation failed: %v", result)
	}
	return result["data"].(map[string]interface{})[name].(map[string]interface{})
}
func TestAuthorizationIdempotencyAndPrivateMessages(t *testing.T) {
	handler := newHandler()
	denied, _ := execute(t, handler, `mutation { setAppreciation(id:"note-0",liked:true) { id } }`, nil)
	if denied["errors"] == nil {
		t.Fatal("anonymous mutation was accepted")
	}
	first := registerAccount(t, handler, "first")
	second := registerAccount(t, handler, "second")
	query := `mutation { createPost(input:{body:"GraphQL note",topic:"community",requestId:"same-note-request"}) { id } }`
	a, _ := execute(t, handler, query, first)
	b, _ := execute(t, handler, query, first)
	if field(t, a, "createPost")["id"] != field(t, b, "createPost")["id"] {
		t.Fatal("retry duplicated the note")
	}
	conflict, _ := execute(t, handler, strings.Replace(query, "GraphQL note", "Different note", 1), first)
	if conflict["errors"] == nil {
		t.Fatal("conflicting retry was accepted")
	}
	like := `mutation { setAppreciation(id:"note-0",liked:true) { likes liked } }`
	execute(t, handler, like, first)
	repeated, _ := execute(t, handler, like, first)
	if field(t, repeated, "setAppreciation")["likes"] != float64(1) {
		t.Fatal("intended reaction toggled or duplicated")
	}
	sent, _ := execute(t, handler, `mutation { sendMessage(input:{threadId:"thread-mika",body:"private body",requestId:"same-message-request"}) { id } }`, first)
	field(t, sent, "sendMessage")
	read := `query { conversation(id:"thread-mika") { messages { body } } }`
	private, _ := execute(t, handler, read, first)
	other, _ := execute(t, handler, read, second)
	if len(field(t, private, "conversation")["messages"].([]interface{})) != 2 || len(field(t, other, "conversation")["messages"].([]interface{})) != 1 {
		t.Fatal("conversation identity leaked across requests")
	}
	execute(t, handler, `mutation { logout }`, first)
	loggedOut, _ := execute(t, handler, `query { viewer { id } settings { email } }`, first)
	data := loggedOut["data"].(map[string]interface{})
	if data["viewer"] != nil || data["settings"] != nil {
		t.Fatal("revoked session remained valid")
	}
}
func TestBodyAndMethodBounds(t *testing.T) {
	handler := newHandler()
	for _, request := range []*http.Request{httptest.NewRequest("GET", "/", nil), httptest.NewRequest("POST", "/", strings.NewReader(strings.Repeat("x", 70000)))} {
		request.Header.Set("Content-Type", "application/json")
		response := httptest.NewRecorder()
		handler.ServeHTTP(response, request)
		if response.Code < 400 {
			t.Fatalf("unbounded request accepted: %d", response.Code)
		}
	}
}
