// Package public provides primitives to interact with the openapi HTTP API.
//
// Code generated by github.com/algorand/oapi-codegen DO NOT EDIT.
package public

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
	// Get a list of unconfirmed transactions currently in the transaction pool by address.
	// (GET /v2/accounts/{address}/transactions/pending)
	GetPendingTransactionsByAddress(ctx echo.Context, address string, params GetPendingTransactionsByAddressParams) error
	// Broadcasts a raw transaction or transaction group to the network.
	// (POST /v2/transactions)
	RawTransaction(ctx echo.Context) error
	// Get a list of unconfirmed transactions currently in the transaction pool.
	// (GET /v2/transactions/pending)
	GetPendingTransactions(ctx echo.Context, params GetPendingTransactionsParams) error
	// Get a specific pending transaction.
	// (GET /v2/transactions/pending/{txid})
	PendingTransactionInformation(ctx echo.Context, txid string, params PendingTransactionInformationParams) error
}

// ServerInterfaceWrapper converts echo contexts to parameters.
type ServerInterfaceWrapper struct {
	Handler ServerInterface
}

// GetPendingTransactionsByAddress converts echo context to params.
func (w *ServerInterfaceWrapper) GetPendingTransactionsByAddress(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "address" -------------
	var address string

	err = runtime.BindStyledParameterWithLocation("simple", false, "address", runtime.ParamLocationPath, ctx.Param("address"), &address)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter address: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params GetPendingTransactionsByAddressParams
	// ------------- Optional query parameter "max" -------------

	err = runtime.BindQueryParameter("form", true, false, "max", ctx.QueryParams(), &params.Max)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter max: %s", err))
	}

	// ------------- Optional query parameter "format" -------------

	err = runtime.BindQueryParameter("form", true, false, "format", ctx.QueryParams(), &params.Format)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter format: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GetPendingTransactionsByAddress(ctx, address, params)
	return err
}

// RawTransaction converts echo context to params.
func (w *ServerInterfaceWrapper) RawTransaction(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.RawTransaction(ctx)
	return err
}

// GetPendingTransactions converts echo context to params.
func (w *ServerInterfaceWrapper) GetPendingTransactions(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params GetPendingTransactionsParams
	// ------------- Optional query parameter "max" -------------

	err = runtime.BindQueryParameter("form", true, false, "max", ctx.QueryParams(), &params.Max)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter max: %s", err))
	}

	// ------------- Optional query parameter "format" -------------

	err = runtime.BindQueryParameter("form", true, false, "format", ctx.QueryParams(), &params.Format)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter format: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GetPendingTransactions(ctx, params)
	return err
}

// PendingTransactionInformation converts echo context to params.
func (w *ServerInterfaceWrapper) PendingTransactionInformation(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "txid" -------------
	var txid string

	err = runtime.BindStyledParameterWithLocation("simple", false, "txid", runtime.ParamLocationPath, ctx.Param("txid"), &txid)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter txid: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params PendingTransactionInformationParams
	// ------------- Optional query parameter "format" -------------

	err = runtime.BindQueryParameter("form", true, false, "format", ctx.QueryParams(), &params.Format)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter format: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.PendingTransactionInformation(ctx, txid, params)
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

	router.GET(baseURL+"/v2/accounts/:address/transactions/pending", wrapper.GetPendingTransactionsByAddress, m...)
	router.POST(baseURL+"/v2/transactions", wrapper.RawTransaction, m...)
	router.GET(baseURL+"/v2/transactions/pending", wrapper.GetPendingTransactions, m...)
	router.GET(baseURL+"/v2/transactions/pending/:txid", wrapper.PendingTransactionInformation, m...)

}

// Base64 encoded, gzipped, json marshaled Swagger object
var swaggerSpec = []string{

	"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",
	"LXylJgEA",
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
