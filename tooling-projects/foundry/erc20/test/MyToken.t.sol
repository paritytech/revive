// SPDX-License-Identifier: MIT
pragma solidity ^0.8.22;

import { Test } from "forge-std/Test.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { MyToken } from "../src/MyToken.sol";

contract MyTokenTest is Test {
    MyToken internal token;
    address internal owner = address(this);
    address internal alice = address(0xA11CE);
    address internal bob   = address(0xB0B);

    function setUp() public {
        token = new MyToken(owner);
    }

    function test_NameAndSymbol() public view {
        assertEq(token.name(), "MyToken");
        assertEq(token.symbol(), "MTK");
    }

    function test_Owner() public view {
        assertEq(token.owner(), owner);
    }

    function test_OwnerCanMint() public {
        token.mint(alice, 1000);
        assertEq(token.balanceOf(alice), 1000);
    }

    function test_NonOwnerCannotMint() public {
        vm.prank(alice);
        vm.expectRevert(
            abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, alice)
        );
        token.mint(alice, 1000);
    }

    function test_TotalSupplyIncreases() public {
        uint256 before = token.totalSupply();
        token.mint(alice, 500);
        assertEq(token.totalSupply() - before, 500);
    }

    function test_Transfer() public {
        token.mint(owner, 1000);
        token.transfer(alice, 100);
        assertEq(token.balanceOf(alice), 100);
        assertEq(token.balanceOf(owner), 900);
    }

    function test_TransferFrom() public {
        token.mint(owner, 1000);
        token.approve(alice, 50);
        vm.prank(alice);
        token.transferFrom(owner, bob, 50);
        assertEq(token.balanceOf(bob), 50);
        assertEq(token.allowance(owner, alice), 0);
    }
}
