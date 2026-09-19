// A local demonstration service. The uf application only knows its GraphQL
// contract; replace this process with an independently operated backend.
package main

import (
	"context"
	_ "embed"
	"encoding/json"
	"log"
	"net/http"
	"os"
	"strings"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
)

//go:embed schema.graphql
var schemaText string

type requestKey struct{}
type requestContext struct {
	writer  http.ResponseWriter
	request *http.Request
}

func newHandler() http.Handler {
	schema := graphql.MustParseSchema(schemaText, newStore(), graphql.UseFieldResolvers(), graphql.MaxDepth(12), graphql.MaxParallelism(16))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Cache-Control", "private, no-store")
		if r.Method != http.MethodPost || !strings.HasPrefix(r.Header.Get("Content-Type"), "application/json") {
			http.Error(w, "POST application/json required", http.StatusUnsupportedMediaType)
			return
		}
		var input struct {
			Query         string                 `json:"query"`
			OperationName string                 `json:"operationName"`
			Variables     map[string]interface{} `json:"variables"`
		}
		if json.NewDecoder(http.MaxBytesReader(w, r.Body, 65536)).Decode(&input) != nil {
			http.Error(w, "invalid GraphQL request", http.StatusBadRequest)
			return
		}
		ctx, cancel := context.WithTimeout(r.Context(), 10*time.Second)
		defer cancel()
		ctx = context.WithValue(ctx, requestKey{}, &requestContext{w, r})
		_ = json.NewEncoder(w).Encode(schema.Exec(ctx, input.Query, input.OperationName, input.Variables))
	})
}

func main() {
	address := os.Getenv("SNS_GRAPHQL_LISTEN")
	if address == "" {
		address = "127.0.0.1:4183"
	}
	server := &http.Server{Addr: address, Handler: newHandler(), ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 15 * time.Second, WriteTimeout: 15 * time.Second}
	log.Printf("Commonplace demonstration GraphQL service: http://%s", address)
	log.Fatal(server.ListenAndServe())
}
