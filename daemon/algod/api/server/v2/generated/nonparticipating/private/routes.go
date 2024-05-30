// Package private provides primitives to interact with the openapi HTTP API.
//
// Code generated by github.com/algorand/oapi-codegen DO NOT EDIT.
package private

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
	// Aborts a catchpoint catchup.
	// (DELETE /v2/catchup/{catchpoint})
	AbortCatchup(ctx echo.Context, catchpoint string) error
	// Starts a catchpoint catchup.
	// (POST /v2/catchup/{catchpoint})
	StartCatchup(ctx echo.Context, catchpoint string, params StartCatchupParams) error

	// (POST /v2/shutdown)
	ShutdownNode(ctx echo.Context, params ShutdownNodeParams) error
}

// ServerInterfaceWrapper converts echo contexts to parameters.
type ServerInterfaceWrapper struct {
	Handler ServerInterface
}

// AbortCatchup converts echo context to params.
func (w *ServerInterfaceWrapper) AbortCatchup(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "catchpoint" -------------
	var catchpoint string

	err = runtime.BindStyledParameterWithLocation("simple", false, "catchpoint", runtime.ParamLocationPath, ctx.Param("catchpoint"), &catchpoint)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter catchpoint: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.AbortCatchup(ctx, catchpoint)
	return err
}

// StartCatchup converts echo context to params.
func (w *ServerInterfaceWrapper) StartCatchup(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "catchpoint" -------------
	var catchpoint string

	err = runtime.BindStyledParameterWithLocation("simple", false, "catchpoint", runtime.ParamLocationPath, ctx.Param("catchpoint"), &catchpoint)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter catchpoint: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params StartCatchupParams
	// ------------- Optional query parameter "min" -------------

	err = runtime.BindQueryParameter("form", true, false, "min", ctx.QueryParams(), &params.Min)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter min: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.StartCatchup(ctx, catchpoint, params)
	return err
}

// ShutdownNode converts echo context to params.
func (w *ServerInterfaceWrapper) ShutdownNode(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params ShutdownNodeParams
	// ------------- Optional query parameter "timeout" -------------

	err = runtime.BindQueryParameter("form", true, false, "timeout", ctx.QueryParams(), &params.Timeout)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter timeout: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.ShutdownNode(ctx, params)
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

	router.DELETE(baseURL+"/v2/catchup/:catchpoint", wrapper.AbortCatchup, m...)
	router.POST(baseURL+"/v2/catchup/:catchpoint", wrapper.StartCatchup, m...)
	router.POST(baseURL+"/v2/shutdown", wrapper.ShutdownNode, m...)

}

// Base64 encoded, gzipped, json marshaled Swagger object
var swaggerSpec = []string{

	"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",
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
