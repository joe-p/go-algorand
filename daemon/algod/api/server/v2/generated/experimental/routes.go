// Package experimental provides primitives to interact with the openapi HTTP API.
//
// Code generated by github.com/algorand/oapi-codegen DO NOT EDIT.
package experimental

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
	// Get a list of assets held by an account, inclusive of asset params.
	// (GET /v2/accounts/{address}/assets)
	AccountAssetsInformation(ctx echo.Context, address string, params AccountAssetsInformationParams) error
	// Returns OK if experimental API is enabled.
	// (GET /v2/experimental)
	ExperimentalCheck(ctx echo.Context) error
	// Fast track for broadcasting a raw transaction or transaction group to the network through the tx handler without performing most of the checks and reporting detailed errors. Should be only used for development and performance testing.
	// (POST /v2/transactions/async)
	RawTransactionAsync(ctx echo.Context) error
}

// ServerInterfaceWrapper converts echo contexts to parameters.
type ServerInterfaceWrapper struct {
	Handler ServerInterface
}

// AccountAssetsInformation converts echo context to params.
func (w *ServerInterfaceWrapper) AccountAssetsInformation(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "address" -------------
	var address string

	err = runtime.BindStyledParameterWithLocation("simple", false, "address", runtime.ParamLocationPath, ctx.Param("address"), &address)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter address: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params AccountAssetsInformationParams
	// ------------- Optional query parameter "limit" -------------

	err = runtime.BindQueryParameter("form", true, false, "limit", ctx.QueryParams(), &params.Limit)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter limit: %s", err))
	}

	// ------------- Optional query parameter "next" -------------

	err = runtime.BindQueryParameter("form", true, false, "next", ctx.QueryParams(), &params.Next)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter next: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.AccountAssetsInformation(ctx, address, params)
	return err
}

// ExperimentalCheck converts echo context to params.
func (w *ServerInterfaceWrapper) ExperimentalCheck(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.ExperimentalCheck(ctx)
	return err
}

// RawTransactionAsync converts echo context to params.
func (w *ServerInterfaceWrapper) RawTransactionAsync(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.RawTransactionAsync(ctx)
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

	router.GET(baseURL+"/v2/accounts/:address/assets", wrapper.AccountAssetsInformation, m...)
	router.GET(baseURL+"/v2/experimental", wrapper.ExperimentalCheck, m...)
	router.POST(baseURL+"/v2/transactions/async", wrapper.RawTransactionAsync, m...)

}

// Base64 encoded, gzipped, json marshaled Swagger object
var swaggerSpec = []string{

	"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",
	"/zsAAP//LJ+iz7QOAQA=",
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
