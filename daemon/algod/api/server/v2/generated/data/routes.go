// Package data provides primitives to interact with the openapi HTTP API.
//
// Code generated by github.com/algorand/oapi-codegen DO NOT EDIT.
package data

import (
	"bytes"
	"compress/gzip"
	"encoding/base64"
	"fmt"
	"net/http"
	"net/url"
	"path"
	"strings"

	. "github.com/algorand/go-algorand/daemon/algod/api/server/v2/generated/model"
	"github.com/algorand/oapi-codegen/pkg/runtime"
	"github.com/getkin/kin-openapi/openapi3"
	"github.com/labstack/echo/v4"
)

// ServerInterface represents all server handlers.
type ServerInterface interface {
	// Removes minimum sync round restriction from the ledger.
	// (DELETE /v2/ledger/sync)
	UnsetSyncRound(ctx echo.Context) error
	// Returns the minimum sync round the ledger is keeping in cache.
	// (GET /v2/ledger/sync)
	GetSyncRound(ctx echo.Context) error
	// Given a round, tells the ledger to keep that round in its cache.
	// (POST /v2/ledger/sync/{round})
	SetSyncRound(ctx echo.Context, round uint64) error
}

// ServerInterfaceWrapper converts echo contexts to parameters.
type ServerInterfaceWrapper struct {
	Handler ServerInterface
}

// UnsetSyncRound converts echo context to params.
func (w *ServerInterfaceWrapper) UnsetSyncRound(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.UnsetSyncRound(ctx)
	return err
}

// GetSyncRound converts echo context to params.
func (w *ServerInterfaceWrapper) GetSyncRound(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GetSyncRound(ctx)
	return err
}

// SetSyncRound converts echo context to params.
func (w *ServerInterfaceWrapper) SetSyncRound(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "round" -------------
	var round uint64

	err = runtime.BindStyledParameterWithLocation("simple", false, "round", runtime.ParamLocationPath, ctx.Param("round"), &round)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter round: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.SetSyncRound(ctx, round)
	return err
}

// This is a simple interface which specifies echo.Route addition functions which
// are present on both echo.Echo and echo.Group, since we want to allow using
// either of them for path registration
type EchoRouter interface {
	CONNECT(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	DELETE(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	GET(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	HEAD(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	OPTIONS(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	PATCH(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	POST(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	PUT(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	TRACE(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
}

// RegisterHandlers adds each server route to the EchoRouter.
func RegisterHandlers(router EchoRouter, si ServerInterface, m ...echo.MiddlewareFunc) {
	RegisterHandlersWithBaseURL(router, si, "", m...)
}

// Registers handlers, and prepends BaseURL to the paths, so that the paths
// can be served under a prefix.
func RegisterHandlersWithBaseURL(router EchoRouter, si ServerInterface, baseURL string, m ...echo.MiddlewareFunc) {

	wrapper := ServerInterfaceWrapper{
		Handler: si,
	}

	router.DELETE(baseURL+"/v2/ledger/sync", wrapper.UnsetSyncRound, m...)
	router.GET(baseURL+"/v2/ledger/sync", wrapper.GetSyncRound, m...)
	router.POST(baseURL+"/v2/ledger/sync/:round", wrapper.SetSyncRound, m...)

}

// Base64 encoded, gzipped, json marshaled Swagger object
var swaggerSpec = []string{

	"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",
	"D/83AAD//+iVc4OCDQEA",
}

// GetSwagger returns the content of the embedded swagger specification file
// or error if failed to decode
func decodeSpec() ([]byte, error) {
	zipped, err := base64.StdEncoding.DecodeString(strings.Join(swaggerSpec, ""))
	if err != nil {
		return nil, fmt.Errorf("error base64 decoding spec: %s", err)
	}
	zr, err := gzip.NewReader(bytes.NewReader(zipped))
	if err != nil {
		return nil, fmt.Errorf("error decompressing spec: %s", err)
	}
	var buf bytes.Buffer
	_, err = buf.ReadFrom(zr)
	if err != nil {
		return nil, fmt.Errorf("error decompressing spec: %s", err)
	}

	return buf.Bytes(), nil
}

var rawSpec = decodeSpecCached()

// a naive cached of a decoded swagger spec
func decodeSpecCached() func() ([]byte, error) {
	data, err := decodeSpec()
	return func() ([]byte, error) {
		return data, err
	}
}

// Constructs a synthetic filesystem for resolving external references when loading openapi specifications.
func PathToRawSpec(pathToFile string) map[string]func() ([]byte, error) {
	var res = make(map[string]func() ([]byte, error))
	if len(pathToFile) > 0 {
		res[pathToFile] = rawSpec
	}

	return res
}

// GetSwagger returns the Swagger specification corresponding to the generated code
// in this file. The external references of Swagger specification are resolved.
// The logic of resolving external references is tightly connected to "import-mapping" feature.
// Externally referenced files must be embedded in the corresponding golang packages.
// Urls can be supported but this task was out of the scope.
func GetSwagger() (swagger *openapi3.T, err error) {
	var resolvePath = PathToRawSpec("")

	loader := openapi3.NewLoader()
	loader.IsExternalRefsAllowed = true
	loader.ReadFromURIFunc = func(loader *openapi3.Loader, url *url.URL) ([]byte, error) {
		var pathToFile = url.String()
		pathToFile = path.Clean(pathToFile)
		getSpec, ok := resolvePath[pathToFile]
		if !ok {
			err1 := fmt.Errorf("path not found: %s", pathToFile)
			return nil, err1
		}
		return getSpec()
	}
	var specData []byte
	specData, err = rawSpec()
	if err != nil {
		return
	}
	swagger, err = loader.LoadFromData(specData)
	if err != nil {
		return
	}
	return
}
