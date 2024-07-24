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
	"YeIjAQA=",
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
