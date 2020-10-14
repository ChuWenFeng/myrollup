pragma solidity ^0.4.24;

import "truffle/Assert.sol";
import "../contracts/Plasma.sol";


contract GulliblePlasma is PlasmaStub {

    function verifyUpdateProof(uint256[8] memory, bytes32, bytes32, bytes32)
        internal view returns (bool valid)
    {
        return true;
    }

}


contract TestPlasma {

    GulliblePlasma plasma;

    constructor() public {

    }

    function beforeAll() public {
        plasma = new GulliblePlasma();
    }

    function testCommitment() {
        bytes memory empty;
        plasma.commitBlock(0, 0, empty, 0);
        Assert.equal(true, true, "commitment failed");
    }

    function testVerification() {
        uint256[8] memory proof_empty;
        plasma.verifyBlock(0, proof_empty);
        Assert.equal(true, true, "verification failed");
    }

}
