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

	// (GET /debug/settings/pprof)
	GetDebugSettingsProf(ctx echo.Context) error

	// (PUT /debug/settings/pprof)
	PutDebugSettingsProf(ctx echo.Context) error
	// Return a list of participation keys
	// (GET /v2/participation)
	GetParticipationKeys(ctx echo.Context) error
	// Add a participation key to the node
	// (POST /v2/participation)
	AddParticipationKey(ctx echo.Context) error
	// Generate and install participation keys to the node.
	// (POST /v2/participation/generate/{address})
	GenerateParticipationKeys(ctx echo.Context, address string, params GenerateParticipationKeysParams) error
	// Delete a given participation key by ID
	// (DELETE /v2/participation/{participation-id})
	DeleteParticipationKeyByID(ctx echo.Context, participationId string) error
	// Get participation key info given a participation ID
	// (GET /v2/participation/{participation-id})
	GetParticipationKeyByID(ctx echo.Context, participationId string) error
	// Append state proof keys to a participation key
	// (POST /v2/participation/{participation-id})
	AppendKeys(ctx echo.Context, participationId string) error
}

// ServerInterfaceWrapper converts echo contexts to parameters.
type ServerInterfaceWrapper struct {
	Handler ServerInterface
}

// GetDebugSettingsProf converts echo context to params.
func (w *ServerInterfaceWrapper) GetDebugSettingsProf(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GetDebugSettingsProf(ctx)
	return err
}

// PutDebugSettingsProf converts echo context to params.
func (w *ServerInterfaceWrapper) PutDebugSettingsProf(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.PutDebugSettingsProf(ctx)
	return err
}

// GetParticipationKeys converts echo context to params.
func (w *ServerInterfaceWrapper) GetParticipationKeys(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GetParticipationKeys(ctx)
	return err
}

// AddParticipationKey converts echo context to params.
func (w *ServerInterfaceWrapper) AddParticipationKey(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.AddParticipationKey(ctx)
	return err
}

// GenerateParticipationKeys converts echo context to params.
func (w *ServerInterfaceWrapper) GenerateParticipationKeys(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "address" -------------
	var address string

	err = runtime.BindStyledParameterWithLocation("simple", false, "address", runtime.ParamLocationPath, ctx.Param("address"), &address)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter address: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params GenerateParticipationKeysParams
	// ------------- Optional query parameter "dilution" -------------

	err = runtime.BindQueryParameter("form", true, false, "dilution", ctx.QueryParams(), &params.Dilution)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter dilution: %s", err))
	}

	// ------------- Required query parameter "first" -------------

	err = runtime.BindQueryParameter("form", true, true, "first", ctx.QueryParams(), &params.First)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter first: %s", err))
	}

	// ------------- Required query parameter "last" -------------

	err = runtime.BindQueryParameter("form", true, true, "last", ctx.QueryParams(), &params.Last)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter last: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GenerateParticipationKeys(ctx, address, params)
	return err
}

// DeleteParticipationKeyByID converts echo context to params.
func (w *ServerInterfaceWrapper) DeleteParticipationKeyByID(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "participation-id" -------------
	var participationId string

	err = runtime.BindStyledParameterWithLocation("simple", false, "participation-id", runtime.ParamLocationPath, ctx.Param("participation-id"), &participationId)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter participation-id: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.DeleteParticipationKeyByID(ctx, participationId)
	return err
}

// GetParticipationKeyByID converts echo context to params.
func (w *ServerInterfaceWrapper) GetParticipationKeyByID(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "participation-id" -------------
	var participationId string

	err = runtime.BindStyledParameterWithLocation("simple", false, "participation-id", runtime.ParamLocationPath, ctx.Param("participation-id"), &participationId)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter participation-id: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GetParticipationKeyByID(ctx, participationId)
	return err
}

// AppendKeys converts echo context to params.
func (w *ServerInterfaceWrapper) AppendKeys(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "participation-id" -------------
	var participationId string

	err = runtime.BindStyledParameterWithLocation("simple", false, "participation-id", runtime.ParamLocationPath, ctx.Param("participation-id"), &participationId)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter participation-id: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.AppendKeys(ctx, participationId)
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

	router.GET(baseURL+"/debug/settings/pprof", wrapper.GetDebugSettingsProf, m...)
	router.PUT(baseURL+"/debug/settings/pprof", wrapper.PutDebugSettingsProf, m...)
	router.GET(baseURL+"/v2/participation", wrapper.GetParticipationKeys, m...)
	router.POST(baseURL+"/v2/participation", wrapper.AddParticipationKey, m...)
	router.POST(baseURL+"/v2/participation/generate/:address", wrapper.GenerateParticipationKeys, m...)
	router.DELETE(baseURL+"/v2/participation/:participation-id", wrapper.DeleteParticipationKeyByID, m...)
	router.GET(baseURL+"/v2/participation/:participation-id", wrapper.GetParticipationKeyByID, m...)
	router.POST(baseURL+"/v2/participation/:participation-id", wrapper.AppendKeys, m...)

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
